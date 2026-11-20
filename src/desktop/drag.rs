use std::cell::{Cell, RefCell};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::*;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::*;
use windows::core::{BOOL, HRESULT, Ref, Result, implement};

use crate::directory::Entry;
use crate::layout::{RECYCLE, full_path, path_key};

use super::panel::{self, DropPlan};
use super::platform::{Pidl, client_point, cursor, namespace_pidl};
use super::{Action, dispatch};

thread_local! {
    static INTERNAL: Cell<bool> = const { Cell::new(false) };
    static SHELL_ITEM: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

struct Internal;

impl Drop for Internal {
    fn drop(&mut self) {
        INTERNAL.with(|active| active.set(false));
        SHELL_ITEM.with(|item| item.borrow_mut().take());
    }
}

#[implement(IDropSource)]
struct Source {
    released: Rc<Cell<Option<POINT>>>,
    preview: Option<panel::Preview>,
}

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        if !keys.contains(MK_LBUTTON) {
            self.released.set(Some(cursor()));
            return DRAGDROP_S_DROP;
        }
        S_OK
    }

    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        if let Some(preview) = &self.preview {
            preview.follow_pointer();
        }
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

pub fn start(hwnd: HWND, entry: &Entry) -> Result<(DROPEFFECT, Option<POINT>)> {
    let released = Rc::new(Cell::new(None));
    let data = source_data(hwnd, &entry.path)?;
    let source: IDropSource = Source {
        released: released.clone(),
        preview: panel::Preview::new(entry, hwnd),
    }
    .into();
    INTERNAL.with(|active| active.set(true));
    if entry.is_namespace() {
        SHELL_ITEM.with(|item| *item.borrow_mut() = Some(entry.path.clone()));
    }
    let _active = Internal;
    let allowed = if entry.is_namespace() {
        DROPEFFECT_LINK
    } else {
        DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK
    };
    let mut effect = DROPEFFECT_NONE;
    let status = unsafe { DoDragDrop(&data, &source, allowed, &mut effect) };
    status.ok()?;
    Ok((effect, released.get()))
}

fn source_data(hwnd: HWND, path: &Path) -> Result<IDataObject> {
    let pidl = if crate::layout::namespace_id(path).is_some() {
        namespace_pidl(path)?
    } else {
        Pidl::parse(path.as_os_str())?
    };
    unsafe {
        let mut child = std::ptr::null_mut();
        let parent: IShellFolder = SHBindToParent(pidl.0, Some(&mut child))?;
        parent.GetUIObjectOf(hwnd, &[child.cast_const()], None)
    }
}

fn paths(data: &IDataObject) -> Vec<PathBuf> {
    if let Some(path) = SHELL_ITEM.with(|item| item.borrow().clone()) {
        return vec![path];
    }
    let format = FORMATETC {
        cfFormat: CF_HDROP.0,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
        ..Default::default()
    };
    unsafe {
        let Ok(mut medium) = data.GetData(&format) else {
            return Vec::new();
        };
        let handle = HDROP(medium.u.hGlobal.0);
        let len = DragQueryFileW(handle, u32::MAX, None);
        let mut files = Vec::new();
        for i in 0..len {
            let size = DragQueryFileW(handle, i, None);
            if size == 0 {
                continue;
            }
            let mut buffer = vec![0; size as usize + 1];
            let size = DragQueryFileW(handle, i, Some(&mut buffer));
            if let Some(path) = full_path(Path::new(&std::ffi::OsString::from_wide(
                &buffer[..size as usize],
            ))) {
                files.push(path);
            }
        }
        ReleaseStgMedium(&mut medium);
        files
    }
}

fn can_drop(folder: &Path, paths: &[PathBuf], keys: MODIFIERKEYS_FLAGS) -> bool {
    if paths.is_empty() || paths.iter().any(|path| !path.is_absolute()) {
        return false;
    }
    if folder == Path::new(RECYCLE) {
        return true;
    }
    let folder = path_key(folder);
    let folder = folder.trim_end_matches('\\');
    paths.iter().all(|source| {
        let key = path_key(source);
        let key = key.trim_end_matches('\\');
        let nested = folder
            .strip_prefix(key)
            .is_some_and(|suffix| suffix.starts_with('\\'));
        let same_parent = source
            .parent()
            .is_some_and(|parent| path_key(parent).trim_end_matches('\\') == folder);
        folder != key
            && !nested
            && (!same_parent || (keys & (MK_CONTROL | MODIFIERKEYS_FLAGS(32))).0 != 0)
    })
}

fn shell_target(owner: HWND, path: &Path) -> Result<IDropTarget> {
    let name = if path == Path::new(RECYCLE) {
        Path::new("::{645FF040-5081-101B-9F08-00AA002F954E}")
    } else {
        path
    };
    let pidl = Pidl::parse(name.as_os_str())?;
    unsafe {
        let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
        let parent: IShellFolder = SHBindToParent(pidl.0, Some(&mut child))?;
        parent.GetUIObjectOf(owner, &[child.cast_const()], None)
    }
}

#[derive(Default)]
struct Session {
    data: Option<IDataObject>,
    paths: Vec<PathBuf>,
    target: Option<(PathBuf, IDropTarget)>,
}

impl Session {
    fn leave(&mut self) {
        if let Some((_, target)) = self.target.take() {
            unsafe {
                let _ = target.DragLeave();
            }
        }
    }
}

#[implement(IDropTarget)]
struct Target {
    hwnd: HWND,
    session: RefCell<Session>,
}

impl Target {
    fn transfer(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        at: &POINTL,
        allowed: DROPEFFECT,
        drop: bool,
    ) -> DROPEFFECT {
        let Some(panel) = panel::lookup(self.hwnd) else {
            return DROPEFFECT_NONE;
        };
        let internal = INTERNAL.with(Cell::get);
        let point = client_point(self.hwnd, POINT { x: at.x, y: at.y });
        let plan = panel.borrow().plan(point, internal);
        let Ok(mut session) = self.session.try_borrow_mut() else {
            return DROPEFFECT_NONE;
        };
        let mut mark = None;
        let effect = match plan {
            Some(DropPlan::Shell(path)) if can_drop(&path, &session.paths, keys) => {
                self.shell(&mut session, path, keys, at, allowed, drop)
            }
            Some(DropPlan::Assign {
                group,
                slot,
                entries,
                mark: insertion,
            }) if !session.paths.is_empty()
                && session.paths.iter().all(|path| path.parent().is_some()) =>
            {
                session.leave();
                let effect = if allowed.contains(DROPEFFECT_LINK) {
                    DROPEFFECT_LINK
                } else {
                    allowed & DROPEFFECT_COPY
                };
                if effect != DROPEFFECT_NONE {
                    mark = insertion;
                    if drop {
                        dispatch(Action::Assign(group, session.paths.clone(), entries, slot));
                    }
                }
                effect
            }
            _ => {
                session.leave();
                DROPEFFECT_NONE
            }
        };
        panel.borrow_mut().mark_drop(
            (!drop && effect != DROPEFFECT_NONE).then_some(point),
            mark.filter(|_| !drop),
        );
        if drop {
            session.leave();
            *session = Session::default();
        }
        effect
    }

    fn shell(
        &self,
        session: &mut Session,
        path: PathBuf,
        keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        allowed: DROPEFFECT,
        drop: bool,
    ) -> DROPEFFECT {
        let mut effect = allowed;
        let result = unsafe {
            if session
                .target
                .as_ref()
                .is_some_and(|(current, _)| *current == path)
            {
                session
                    .target
                    .as_ref()
                    .unwrap()
                    .1
                    .DragOver(keys, *point, &mut effect)
            } else {
                session.leave();
                let Some(data) = session.data.as_ref() else {
                    return DROPEFFECT_NONE;
                };
                let Ok(target) = shell_target(self.hwnd, &path) else {
                    return DROPEFFECT_NONE;
                };
                let result = target.DragEnter(data, keys, *point, &mut effect);
                if result.is_ok() {
                    session.target = Some((path, target));
                }
                result
            }
        };
        if result.is_err() {
            session.leave();
            return DROPEFFECT_NONE;
        }
        effect &= allowed;
        if drop
            && effect != DROPEFFECT_NONE
            && let Some((_, target)) = session.target.take()
        {
            let Some(data) = session.data.as_ref() else {
                return DROPEFFECT_NONE;
            };
            effect = allowed;
            if unsafe { target.Drop(data, keys, *point, &mut effect) }.is_err() {
                return DROPEFFECT_NONE;
            }
            dispatch(Action::Refresh);
        }
        effect & allowed
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        at: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.leave();
            *session = Session {
                data: data.as_ref().cloned(),
                paths: data.as_ref().map_or_else(Vec::new, paths),
                target: None,
            };
        }
        self.DragOver(keys, at, effect)
    }

    fn DragOver(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        at: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        unsafe {
            *effect = self.transfer(keys, at, *effect, false);
        }
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.leave();
            *session = Session::default();
        }
        if let Some(panel) = panel::lookup(self.hwnd) {
            panel.borrow_mut().mark_drop(None, None);
        }
        Ok(())
    }

    fn Drop(
        &self,
        _: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        at: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        unsafe {
            *effect = self.transfer(keys, at, *effect, true);
        }
        Ok(())
    }
}

pub fn register(hwnd: HWND) -> Result<()> {
    let target: IDropTarget = Target {
        hwnd,
        session: RefCell::default(),
    }
    .into();
    unsafe { RegisterDragDrop(hwnd, &target) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Bounds, Group};
    use std::fs;
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;

    #[test]
    fn windows_file_drop_assigns_a_reference_or_copies_to_a_portal() {
        let _ole = super::super::Ole::new().unwrap();
        panel::register_for_test();
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("测试文件.txt");
        let target = dir.path().join("入口");
        fs::write(&source, b"drag test").unwrap();
        fs::create_dir(&target).unwrap();
        let placed = Bounds {
            left: 40,
            top: 40,
            width: 320,
            height: 240,
        };
        let group = Group::new("测试", 0, placed, "desktop");
        let mut spec = panel::Spec {
            active: group.clone(),
            frame: group.clone(),
            placed,
            tabs: Vec::new(),
            folder: None,
            entries: Rc::default(),
            neighbors: Rc::default(),
            status: "",
            loose: false,
        };
        let hwnd = panel::create(HWND::default(), false, spec.clone()).unwrap();
        panel::update(hwnd, spec.clone());
        let data = source_data(hwnd, &source).unwrap();
        let allowed = DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK;
        let mut point = POINT { x: 100, y: 160 };
        assert!(unsafe { ClientToScreen(hwnd, &mut point) }.as_bool());
        let at = POINTL {
            x: point.x,
            y: point.y,
        };
        let drop_target: IDropTarget = Target {
            hwnd,
            session: RefCell::default(),
        }
        .into();
        let mut effect = allowed;
        unsafe {
            drop_target
                .DragEnter(&data, MK_LBUTTON, at, &mut effect)
                .unwrap();
        }
        assert_eq!(effect, DROPEFFECT_LINK);
        effect = allowed;
        unsafe {
            drop_target
                .Drop(&data, MODIFIERKEYS_FLAGS(0), at, &mut effect)
                .unwrap();
        }
        let command = super::super::ACTIONS.with(|queue| queue.borrow_mut().pop_front());
        let Some(Action::Assign(id, files, _, _)) = command else {
            panic!("expected a logical assignment");
        };
        assert_eq!(id, group.id);
        assert_eq!(
            files.iter().map(|path| path_key(path)).collect::<Vec<_>>(),
            [path_key(&full_path(&source).unwrap())]
        );
        assert!(source.exists());

        spec.active.folder = Some(target.clone());
        spec.folder = Some(target.clone());
        panel::update(hwnd, spec);
        let mut effect = allowed;
        unsafe {
            drop_target
                .DragEnter(&data, MK_CONTROL | MK_LBUTTON, at, &mut effect)
                .unwrap();
        }
        assert_eq!(effect, DROPEFFECT_COPY);
        effect = allowed;
        unsafe {
            drop_target
                .Drop(&data, MK_CONTROL, at, &mut effect)
                .unwrap();
        }
        assert_eq!(effect, DROPEFFECT_COPY);
        let copy = target.join(source.file_name().unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !copy.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(fs::read(copy).unwrap(), fs::read(source).unwrap());
        unsafe {
            DestroyWindow(hwnd).unwrap();
        }
    }
}
