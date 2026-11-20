use std::ffi::{OsStr, OsString, c_void};
use std::fs;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, CoCreateInstance, CoTaskMemFree, IServiceProvider,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::*;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    AccessibleChildren, AccessibleObjectFromWindow, IAccessible,
};
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, Interface, PCWSTR, Result, w};

use crate::directory::Entry;
use crate::layout::{Bounds, NAMESPACE_PREFIX, namespace_id};

const CONTROL_PANEL: uuid::Uuid = uuid::uuid!("5399e694-6ce5-4d6c-8fce-1d8870fdcba0");
const CONTROL_PANEL_IDS: [uuid::Uuid; 3] = [
    CONTROL_PANEL,
    uuid::uuid!("21ec2020-3aea-1069-a2dd-08002b30309d"),
    uuid::uuid!("26ee0668-a00a-44d7-9371-beb064c98683"),
];

pub fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain([0]).collect()
}

pub fn instance() -> HINSTANCE {
    // A null module name refers to the already loaded executable.
    unsafe { GetModuleHandleW(None).map_or(HINSTANCE::default(), |module| HINSTANCE(module.0)) }
}

pub fn resource_icon() -> HICON {
    // MAKEINTRESOURCE passes the numeric identifier in the low word of the pointer.
    unsafe {
        LoadIconW(Some(instance()), PCWSTR(std::ptr::without_provenance(1))).unwrap_or_default()
    }
}

pub fn alert(owner: HWND, message: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(wide(message).as_ptr()),
            PCWSTR(wide(tr!("桌面格 · DeskGrid", "桌面格 · DeskGrid", "DeskGrid")).as_ptr()),
            flags,
        )
    }
}

pub fn register(class: PCWSTR, proc: WNDPROC, styles: WNDCLASS_STYLES) -> Result<()> {
    let wnd = WNDCLASSW {
        style: styles,
        lpfnWndProc: proc,
        hInstance: instance(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        hIcon: resource_icon(),
        lpszClassName: class,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&wnd) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(())
}

pub fn dpi(window: HWND) -> i32 {
    let value = unsafe {
        if window.is_invalid() {
            GetDpiForSystem()
        } else {
            GetDpiForWindow(window)
        }
    };
    if value == 0 { 96 } else { value as i32 }
}

pub fn px(value: i32, dpi: i32) -> i32 {
    (value * dpi + 48) / 96
}

pub fn monitor_dpi(point: POINT) -> i32 {
    let mut x = 96;
    let mut y = 96;
    unsafe {
        let _ = GetDpiForMonitor(
            MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST),
            MDT_EFFECTIVE_DPI,
            &mut x,
            &mut y,
        );
    }
    x as i32
}

pub fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x.saturating_add(w),
        bottom: y.saturating_add(h),
    }
}

pub fn bounds(rect: RECT) -> Bounds {
    Bounds {
        left: rect.left,
        top: rect.top,
        width: rect.right.saturating_sub(rect.left),
        height: rect.bottom.saturating_sub(rect.top),
    }
}

pub fn client(window: HWND) -> RECT {
    let mut result = RECT::default();
    unsafe {
        let _ = GetClientRect(window, &mut result);
    }
    result
}

pub fn cursor() -> POINT {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    point
}

pub fn client_point(window: HWND, mut point: POINT) -> POINT {
    unsafe {
        let _ = ScreenToClient(window, &mut point);
    }
    point
}

pub fn mouse_point(value: LPARAM) -> POINT {
    POINT {
        x: value.0 as i16 as i32,
        y: (value.0 >> 16) as i16 as i32,
    }
}

pub fn contains(rect: RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

pub fn work_area(proposed: Option<Bounds>, point: Option<POINT>) -> Bounds {
    let monitor = unsafe {
        if let Some(point) = point {
            MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST)
        } else if let Some(proposed) = proposed {
            MonitorFromRect(
                &rect(
                    proposed.left,
                    proposed.top,
                    proposed.width.max(1),
                    proposed.height.max(1),
                ),
                MONITOR_DEFAULTTONEAREST,
            )
        } else {
            MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY)
        }
    };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        bounds(info.rcWork)
    } else {
        Bounds {
            width: 1024,
            height: 768,
            ..Bounds::default()
        }
    }
}

pub fn known_folder(id: &windows::core::GUID) -> Result<PathBuf> {
    unsafe {
        let allocated = SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, None)?;
        let path = OsString::from_wide(allocated.as_wide());
        CoTaskMemFree(Some(allocated.0.cast()));
        Ok(path.into())
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Explorer {
    pub host: HWND,
    pub list: HWND,
}

impl Explorer {
    pub fn icons(self) -> Option<Vec<RECT>> {
        if self.list.is_invalid() || icon_list_hidden(self.list) {
            return Some(Vec::new());
        }
        unsafe {
            let mut accessible: Option<IAccessible> = None;
            AccessibleObjectFromWindow(
                self.list,
                OBJID_CLIENT.0 as u32,
                &IAccessible::IID,
                (&mut accessible as *mut Option<IAccessible>).cast(),
            )
            .ok()?;
            let accessible = accessible?;
            let count = accessible.accChildCount().ok()?.max(0) as usize;
            if count == 0 {
                return Some(Vec::new());
            }
            let mut children = (0..count).map(|_| VARIANT::default()).collect::<Vec<_>>();
            let mut returned = 0;
            AccessibleChildren(&accessible, 0, &mut children, &mut returned).ok()?;
            let mut icons = Vec::new();
            for child in children.iter().take(returned.max(0) as usize) {
                let (mut x, mut y, mut width, mut height) = (0, 0, 0, 0);
                if accessible
                    .accLocation(&mut x, &mut y, &mut width, &mut height, child)
                    .is_ok()
                    && width > 0
                    && height > 0
                {
                    icons.push(rect(x, y, width, height));
                }
            }
            Some(icons)
        }
    }

    pub fn blank(self, point: POINT, icons: Option<&[RECT]>) -> bool {
        (self.icons_hidden()
            || icons.is_some_and(|icons| !icons.iter().any(|&icon| contains(icon, point))))
            && wallpaper(self.host, point)
    }
}

fn icon_list_hidden(list: HWND) -> bool {
    (unsafe { GetWindowLongW(list, GWL_STYLE) } as u32 & WS_VISIBLE.0) == 0
}

impl Explorer {
    pub fn icons_hidden(self) -> bool {
        !self.list.is_invalid() && icon_list_hidden(self.list)
    }
}

pub fn explorer() -> Explorer {
    unsafe extern "system" fn find(window: HWND, context: LPARAM) -> BOOL {
        let view = unsafe { FindWindowExW(Some(window), None, w!("SHELLDLL_DefView"), None) };
        if let Ok(view) = view
            && unsafe { IsWindowVisible(view) }.as_bool()
        {
            unsafe { *(context.0 as *mut HWND) = view };
            return false.into();
        }
        true.into()
    }
    let mut view = HWND::default();
    unsafe {
        let shell = GetShellWindow();
        if !shell.is_invalid() {
            let _ = find(shell, LPARAM(&mut view as *mut HWND as isize));
        }
        if view.is_invalid() {
            let _ = EnumWindows(Some(find), LPARAM(&mut view as *mut HWND as isize));
        }
        if view.is_invalid() {
            return Explorer::default();
        }
        Explorer {
            host: GetParent(view).unwrap_or_default(),
            list: FindWindowExW(Some(view), None, w!("SysListView32"), None).unwrap_or_default(),
        }
    }
}

fn window_pid(window: HWND) -> u32 {
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut pid));
    }
    pid
}

pub fn wallpaper(host: HWND, point: POINT) -> bool {
    if host.is_invalid() {
        return false;
    }
    unsafe {
        let hit = WindowFromPoint(point);
        if hit.is_invalid() || window_pid(hit) != window_pid(host) {
            return false;
        }
        let root = GetAncestor(hit, GA_ROOT);
        if root == host {
            return true;
        }
        let mut class = [0u16; 32];
        let count = GetClassNameW(root, &mut class);
        matches!(
            String::from_utf16_lossy(&class[..count.max(0) as usize]).as_str(),
            "WorkerW" | "Progman"
        )
    }
}

fn registry_value(
    path: PCWSTR,
    field: PCWSTR,
    flags: REG_ROUTINE_FLAGS,
    bytes: Option<*mut c_void>,
    length: &mut u32,
) -> bool {
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path,
            field,
            flags,
            None,
            bytes,
            Some(length),
        ) == ERROR_SUCCESS
    }
}

fn explorer_hides_icons() -> bool {
    let mut value = 0u32;
    registry_value(
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced"),
        w!("HideIcons"),
        RRF_RT_REG_DWORD,
        Some((&mut value as *mut u32).cast()),
        &mut 4,
    ) && value != 0
}

const STARTUP: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

pub fn autostart() -> bool {
    registry_value(STARTUP, w!("DeskGrid"), RRF_RT_REG_SZ, None, &mut 0)
}

pub fn set_autostart(enabled: bool) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut key = HKEY::default();
    unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, STARTUP, None, KEY_SET_VALUE, &mut key).ok()?;
        let status = if enabled {
            let exe = std::env::current_exe();
            exe.map(|exe| {
                let mut command = OsString::from("\"");
                command.push(exe);
                command.push("\" --autostart");
                let command = wide(command);
                let bytes = std::slice::from_raw_parts(command.as_ptr().cast(), command.len() * 2);
                RegSetValueExW(key, w!("DeskGrid"), None, REG_SZ, Some(bytes))
            })
        } else {
            let status = RegDeleteValueW(key, w!("DeskGrid"));
            Ok(if status == ERROR_FILE_NOT_FOUND {
                ERROR_SUCCESS
            } else {
                status
            })
        };
        let _ = RegCloseKey(key);
        status?.ok()?;
    }
    Ok(())
}

pub struct Handle(pub HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub struct InstanceLock {
    mutex: Handle,
    pub owned: bool,
}

impl InstanceLock {
    pub fn acquire(key: &str, timeout: u32) -> Result<Self> {
        let name = wide(key);
        let mutex = Handle(unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr()))? });
        let status = unsafe { WaitForSingleObject(mutex.0, timeout) };
        if status == WAIT_FAILED {
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self {
            mutex,
            owned: status == WAIT_OBJECT_0 || status == WAIT_ABANDONED,
        })
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        if self.owned {
            unsafe {
                let _ = ReleaseMutex(self.mutex.0);
            }
        }
    }
}

fn process_stamp(handle: HANDLE) -> Option<u64> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }.ok()?;
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

fn window_stamp(hwnd: HWND) -> Option<(u32, u64)> {
    let pid = window_pid(hwnd);
    let process =
        Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
    Some((pid, process_stamp(process.0)?))
}

pub fn restore_legacy_icons(dir: &Path) -> io::Result<()> {
    let session = session_id()?;
    for (suffix, rust_marker) in [("-rust", true), ("", false)] {
        let marker = dir.join(format!("desktop-icons-{session}{suffix}.txt"));
        let text = match fs::read_to_string(&marker) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let list = explorer().list;
        if list.is_invalid() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Explorer desktop not ready",
            ));
        }
        let matches = if rust_marker {
            window_stamp(list).is_some_and(|(pid, stamp)| {
                text.trim() == format!("{} {pid} {stamp}", list.0 as isize)
            })
        } else {
            text.trim().parse::<isize>().ok() == Some(list.0 as isize)
        };
        if matches
            && !explorer_hides_icons()
            && icon_list_hidden(list)
            && !unsafe { ShowWindowAsync(list, SW_SHOWNA) }.as_bool()
        {
            return Err(io::Error::last_os_error());
        }
        fs::remove_file(marker)?;
    }
    Ok(())
}

fn session_id() -> io::Result<u32> {
    let mut session = 0;
    unsafe { ProcessIdToSessionId(std::process::id(), &mut session) }
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(session)
}

pub struct IconGuard {
    dir: PathBuf,
    marker: PathBuf,
    hidden: HWND,
    watchdog: Option<Child>,
}

impl IconGuard {
    pub fn new(dir: &Path) -> io::Result<Self> {
        Ok(Self {
            dir: dir.to_owned(),
            marker: dir.join(format!("desktop-icons-{}-rust.txt", session_id()?)),
            hidden: HWND::default(),
            watchdog: None,
        })
    }

    pub fn update(&mut self, list: HWND, hide: bool) -> io::Result<()> {
        if !self.hidden.is_invalid() && (!hide || list != self.hidden) {
            restore_legacy_icons(&self.dir)?;
            self.hidden = HWND::default();
        }
        if !hide || list.is_invalid() || explorer_hides_icons() || icon_list_hidden(list) {
            return Ok(());
        }
        if self
            .watchdog
            .as_mut()
            .map(Child::try_wait)
            .transpose()?
            .is_none_or(|exit| exit.is_some())
        {
            let stamp = process_stamp(unsafe { GetCurrentProcess() })
                .ok_or_else(io::Error::last_os_error)?;
            self.watchdog = Some(
                Command::new(std::env::current_exe()?)
                    .arg("--restore-icons")
                    .arg(std::process::id().to_string())
                    .arg(stamp.to_string())
                    .spawn()?,
            );
        }
        let (pid, stamp) = window_stamp(list).ok_or_else(io::Error::last_os_error)?;
        let mut pending = tempfile::NamedTempFile::new_in(&self.dir)?;
        use std::io::Write;
        write!(pending, "{} {pid} {stamp}", list.0 as isize)?;
        pending.as_file().sync_all()?;
        pending.persist(&self.marker)?;
        self.hidden = list;
        if !unsafe { ShowWindowAsync(list, SW_HIDE) }.as_bool() {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for IconGuard {
    fn drop(&mut self) {
        if !self.hidden.is_invalid() {
            let _ = restore_legacy_icons(&self.dir);
        }
    }
}

pub fn watch_icons(dir: &Path, key: &str, pid: u32, stamp: u64) {
    if let Ok(process) = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        )
    } {
        let process = Handle(process);
        if process_stamp(process.0) == Some(stamp) {
            unsafe {
                WaitForSingleObject(process.0, INFINITE);
            }
        }
    }
    if let Ok(lock) = InstanceLock::acquire(key, 0)
        && lock.owned
    {
        let _ = restore_legacy_icons(dir);
    }
}

pub struct Pidl(pub *mut ITEMIDLIST);

impl Pidl {
    pub fn parse(path: &OsStr) -> Result<Self> {
        let mut pidl = std::ptr::null_mut();
        unsafe {
            SHParseDisplayName(PCWSTR(wide(path).as_ptr()), None, &mut pidl, 0, None)?;
        }
        Ok(Self(pidl))
    }
}

impl Drop for Pidl {
    fn drop(&mut self) {
        unsafe {
            CoTaskMemFree(Some(self.0.cast()));
        }
    }
}

pub fn desktop_system_items() -> Result<Vec<Entry>> {
    fn name(item: &Pidl, kind: SIGDN) -> Result<OsString> {
        unsafe {
            let value = SHGetNameFromIDList(item.0, kind)?;
            let name = OsString::from_wide(value.as_wide());
            CoTaskMemFree(Some(value.0.cast()));
            Ok(name)
        }
    }

    let enumeration: IEnumIDList = unsafe {
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        let location = VARIANT::default();
        let mut handle = 0;
        let desktop = windows.FindWindowSW(
            &location,
            &location,
            SWC_DESKTOP,
            &mut handle,
            SWFO_NEEDDISPATCH,
        )?;
        let services: IServiceProvider = desktop.cast()?;
        let browser: IShellBrowser = services.QueryService(&SID_STopLevelBrowser)?;
        let view: IFolderView = browser.QueryActiveShellView()?.cast()?;
        if view.ItemCount(SVGIO_ALLVIEW)? == 0 {
            return Ok(Vec::new());
        }
        view.Items(SVGIO_ALLVIEW)?
    };
    let mut items = Vec::new();
    let mut control_panel = None;
    loop {
        let mut next = [std::ptr::null_mut()];
        unsafe { enumeration.Next(&mut next, None) }.ok()?;
        if next[0].is_null() {
            break;
        }
        let item = Pidl(next[0]);
        let Ok(parsing_name) = name(&item, SIGDN_DESKTOPABSOLUTEPARSING) else {
            continue;
        };
        let path = PathBuf::from(format!(
            "{NAMESPACE_PREFIX}{}",
            parsing_name.to_string_lossy()
        ));
        let Some(id) = namespace_id(&path) else {
            continue;
        };
        if id == uuid::uuid!("645ff040-5081-101b-9f08-00aa002f954e") {
            items.push(Entry::recycle());
            continue;
        }
        let Ok(label) = name(&item, SIGDN_NORMALDISPLAY) else {
            continue;
        };
        let entry = Entry::namespace(
            format!("{NAMESPACE_PREFIX}::{{{id}}}").into(),
            label.to_string_lossy().into_owned(),
        );
        if CONTROL_PANEL_IDS.contains(&id) {
            if control_panel.is_none() || id == CONTROL_PANEL {
                control_panel = Some(entry);
            }
        } else {
            items.push(entry);
        }
    }
    items.extend(control_panel);
    Ok(items)
}

pub fn namespace_pidl(path: &Path) -> Result<Pidl> {
    let path = path
        .to_str()
        .and_then(|path| path.strip_prefix(NAMESPACE_PREFIX))
        .ok_or(windows::core::Error::from(E_INVALIDARG))?;
    Pidl::parse(OsStr::new(path))
}

#[cfg(test)]
#[test]
#[ignore = "requires a desktop with This PC and Control Panel and without Home, Gallery or Linux"]
fn desktop_system_items_match_the_live_desktop_view() {
    let _ole = super::Ole::new().unwrap();
    let items = desktop_system_items().unwrap();
    for label in [
        "Home",
        "主文件夹",
        "主資料夾",
        "Gallery",
        "图库",
        "圖庫",
        "Linux",
    ] {
        assert!(
            !items
                .iter()
                .any(|entry| entry.label.eq_ignore_ascii_case(label)),
            "unexpected Explorer entry {label}",
        );
    }
    for candidates in [
        &[uuid::uuid!("20d04fe0-3aea-1069-a2d8-08002b30309d")][..],
        &CONTROL_PANEL_IDS,
    ] {
        let mut matches = items
            .iter()
            .filter(|entry| namespace_id(&entry.path).is_some_and(|id| candidates.contains(&id)));
        let entry = matches
            .next()
            .expect("a configured desktop system icon is missing");
        assert!(!entry.label.is_empty());
        assert!(item_icon(entry, true).is_some());
        assert!(matches.next().is_none(), "duplicate system icon");
    }
}

pub fn open(owner: HWND, path: &Path) -> Result<()> {
    if namespace_id(path).is_some() {
        let item = namespace_pidl(path)?;
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_INVOKEIDLIST | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
            hwnd: owner,
            lpIDList: item.0.cast(),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        return unsafe { ShellExecuteExW(&mut info) };
    }
    let file = wide(path);
    let folder = path
        .extension()
        .filter(|ext| ext.eq_ignore_ascii_case("exe"))
        .and_then(|_| path.parent())
        .map(wide);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        hwnd: owner,
        lpFile: PCWSTR(file.as_ptr()),
        lpDirectory: folder
            .as_ref()
            .map_or(PCWSTR::null(), |folder| PCWSTR(folder.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }
}

pub fn reveal(path: &Path) -> Result<()> {
    let item = Pidl::parse(path.as_os_str())?;
    unsafe { SHOpenFolderAndSelectItems(item.0, None, 0) }
}

pub struct Icon(usize);

impl Icon {
    pub fn handle(&self) -> HICON {
        HICON(self.0 as *mut c_void)
    }
}

impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.handle());
        }
    }
}

pub fn item_icon(entry: &Entry, actual: bool) -> Option<Icon> {
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};

    unsafe {
        if entry.is_recycle() {
            let mut info = SHSTOCKICONINFO {
                cbSize: size_of::<SHSTOCKICONINFO>() as u32,
                ..Default::default()
            };
            SHGetStockIconInfo(SIID_RECYCLER, SHGSI_ICON, &mut info).ok()?;
            return Some(Icon(info.hIcon.0 as usize));
        }
        let mut info = SHFILEINFOW::default();
        if entry.is_namespace() {
            let item = namespace_pidl(&entry.path).ok()?;
            let result = SHGetFileInfoW(
                PCWSTR(item.0.cast()),
                FILE_ATTRIBUTE_NORMAL,
                Some(&mut info),
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_PIDL | SHGFI_ICON,
            );
            return (result != 0 && !info.hIcon.is_invalid()).then(|| Icon(info.hIcon.0 as usize));
        }
        let name = if actual {
            wide(&entry.path)
        } else {
            wide(format!("file.{}", entry.extension))
        };
        let attributes = if entry.folder {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        };
        let flags = if actual {
            SHGFI_ICON
        } else {
            SHGFI_ICON | SHGFI_USEFILEATTRIBUTES
        };
        let result = SHGetFileInfoW(
            PCWSTR(name.as_ptr()),
            attributes,
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            flags,
        );
        (result != 0 && !info.hIcon.is_invalid()).then(|| Icon(info.hIcon.0 as usize))
    }
}

pub fn menu<T>(owner: HWND, point: POINT, items: Vec<(String, Option<T>)>) -> Option<T> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for (i, (label, action)) in items.iter().enumerate() {
            let flags = if action.is_some() {
                MF_STRING
            } else if label.is_empty() {
                MF_SEPARATOR
            } else {
                MF_STRING | MF_GRAYED
            };
            let _ = AppendMenuW(menu, flags, i + 1, PCWSTR(wide(label).as_ptr()));
        }
        let _ = SetForegroundWindow(owner);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            Some(0),
            owner,
            None,
        )
        .0;
        let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        let index = usize::try_from(command).ok()?.checked_sub(1)?;
        items.into_iter().nth(index)?.1
    }
}
