mod dashboard;
mod dialog;
mod drag;
mod gesture;
mod icons;
mod manage;
mod panel;
mod platform;
mod theme;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use windows::Win32::Foundation::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::directory::{Entry, Listing, Scanner};
use crate::layout::{self, Bounds, Group, Layout, Placement, full_path, path_key};

use self::platform::*;

const WAKE: u32 = WM_APP + 1;
const TRAY_MESSAGE: u32 = WM_APP + 2;
const QUICK_HIDE: u32 = WM_APP + 3;
const MAIN_CLASS: windows::core::PCWSTR = w!("DeskGrid.Controller");

enum Action {
    Tick,
    Refresh,
    RestoreTray,
    Manage,
    TrayMenu,
    PanelMenu(HWND, String, String, POINT, Option<Entry>),
    Hidden,
    Peek,
    Search(Option<String>),
    StepPage(isize),
    SwitchPage(String),
    DrawGroup(Bounds),
    NewPage,
    Place(String, Bounds),
    Collapse(String),
    Activate(String),
    Navigate(String, PathBuf),
    Back(String),
    Open(PathBuf),
    Reveal(PathBuf),
    Copy(PathBuf),
    Drag(HWND, Entry),
    Assign(String, Vec<PathBuf>, Rc<Vec<Entry>>, Option<usize>),
    Clear(PathBuf),
    EditGroup(String),
    NewGroup(bool),
    RemoveGroup(String),
    DefaultGroup(String),
    Membership(String),
    Dock(String),
    Detach(String),
    Pages,
    Rules,
    Align,
    Options,
    Import,
    Export,
    Exit,
}

thread_local! {
    static RECEIVER: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    static TICK_PENDING: Cell<bool> = const { Cell::new(false) };
    static ACTIONS: RefCell<VecDeque<Action>> = const { RefCell::new(VecDeque::new()) };
}

fn dispatch(action: Action) {
    ACTIONS.with(|actions| actions.borrow_mut().push_back(action));
    let receiver = RECEIVER.with(Cell::get);
    if !receiver.is_invalid() {
        unsafe {
            let _ = PostMessageW(Some(receiver), WAKE, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn controller(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let action = match message {
        WAKE => return LRESULT(0),
        gesture::UPDATE => {
            gesture::update_preview();
            return LRESULT(0);
        }
        gesture::REPLAY => {
            gesture::replay_click();
            return LRESULT(0);
        }
        WM_TIMER if !TICK_PENDING.with(|pending| pending.replace(true)) => Action::Tick,
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => Action::Refresh,
        QUICK_HIDE => Action::Hidden,
        TRAY_MESSAGE if lp.0 == WM_RBUTTONUP as isize => Action::TrayMenu,
        TRAY_MESSAGE if lp.0 == WM_LBUTTONDBLCLK as isize => Action::Manage,
        WM_HOTKEY => match wp.0 {
            1 => Action::Hidden,
            2 => Action::Peek,
            3 => Action::Search(None),
            4 => Action::StepPage(-1),
            5 => Action::StepPage(1),
            _ => return LRESULT(0),
        },
        WM_CLOSE => Action::Exit,
        WM_ENDSESSION if wp.0 != 0 => {
            unsafe {
                PostQuitMessage(0);
            }
            return LRESULT(0);
        }
        value if value != 0 && value == TASKBAR_CREATED.with(Cell::get) => Action::RestoreTray,
        _ => return unsafe { DefWindowProcW(hwnd, message, wp, lp) },
    };
    dispatch(action);
    LRESULT(0)
}

pub fn report_error(message: &str) {
    alert(HWND::default(), message, MB_OK | MB_ICONERROR);
}

struct Ole;

impl Ole {
    fn new() -> windows::core::Result<Self> {
        unsafe {
            OleInitialize(None)?;
        }
        Ok(Self)
    }
}

impl Drop for Ole {
    fn drop(&mut self) {
        unsafe {
            OleUninitialize();
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var_os("DESKGRID_DATA_DIR")
        .map(PathBuf::from)
        .map_or_else(
            || known_folder(&FOLDERID_LocalAppData).map(|path| path.join("DeskGrid")),
            Ok,
        )?;
    fs::create_dir_all(&data)?;
    let data = fs::canonicalize(data)?;
    let mut hash = DefaultHasher::new();
    path_key(&data).hash(&mut hash);
    let key = format!("Local\\DeskGrid.Rust.{:016x}", hash.finish());
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|value| value == "--restore-icons") {
        if let [_, pid, stamp] = args.as_slice()
            && let (Ok(pid), Ok(stamp)) = (
                pid.to_string_lossy().parse(),
                stamp.to_string_lossy().parse(),
            )
        {
            watch_icons(&data, &key, pid, stamp);
        }
        return Ok(());
    }
    let name = wide(format!("{key}.Open"));
    let signal =
        Handle(unsafe { CreateEventW(None, false, false, windows::core::PCWSTR(name.as_ptr()))? });
    let lock = InstanceLock::acquire(&key, 0)?;
    if !lock.owned {
        unsafe {
            SetEvent(signal.0)?;
        }
        return Ok(());
    }
    let _ole = Ole::new()?;
    register(MAIN_CLASS, Some(controller), WNDCLASS_STYLES(0))?;
    register(panel::CLASS, Some(panel::proc), CS_DBLCLKS)?;
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            MAIN_CLASS,
            w!("DeskGrid"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance()),
            None,
        )?
    };
    RECEIVER.with(|receiver| receiver.set(hwnd));
    TASKBAR_CREATED
        .with(|event| event.set(unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }));
    let mut app = App::new(hwnd, &data, signal)?;
    app.tray();
    for (id, key) in [
        (1, u32::from(b'H')),
        (2, VK_SPACE.0 as u32),
        (3, u32::from(b'F')),
        (4, VK_PRIOR.0 as u32),
        (5, VK_NEXT.0 as u32),
    ] {
        unsafe {
            let _ = RegisterHotKey(Some(hwnd), id, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, key);
        }
    }
    if unsafe { SetTimer(Some(hwnd), 1, 250, None) } == 0 {
        return Err(windows::core::Error::from_thread().into());
    }
    app.tick(true);
    if !args.iter().any(|arg| arg == "--autostart") {
        dispatch(Action::Manage);
    }
    let mut msg = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
        if result < 0 {
            return Err(windows::core::Error::from_thread().into());
        }
        if result == 0 {
            break;
        }
        unsafe {
            let manager = dashboard::window();
            if manager.is_invalid() || !IsDialogMessageW(manager, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        while let Some(action) = ACTIONS.with(|actions| actions.borrow_mut().pop_front()) {
            if !app.handle(action) {
                return Ok(());
            }
        }
    }
    Ok(())
}

struct Tray(NOTIFYICONDATAW);

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.0);
        }
    }
}

#[derive(Clone)]
struct SearchRow {
    group: Option<String>,
    folder: Option<PathBuf>,
    entry: Entry,
    description: String,
}

struct App {
    hwnd: HWND,
    signal: Handle,
    tray_icon: Option<Tray>,
    explorer: Explorer,
    desktop_input: Option<gesture::DesktopInput>,
    native_icons: Option<Rc<[RECT]>>,
    icons: IconGuard,
    icon_recovery_pending: bool,
    reported_icon_error: bool,
    path: PathBuf,
    has_valid_primary: bool,
    preserved_invalid_primary: bool,
    save_pending: bool,
    reported_save_error: bool,
    next_save_retry: Instant,
    layout: Layout,
    windows: HashMap<String, HWND>,
    loose: HashMap<String, HWND>,
    scanner: Scanner,
    desktop_folders: Vec<PathBuf>,
    desktop_keys: HashSet<String>,
    shell_items: Vec<Entry>,
    desktop_ready: bool,
    desktop_status: &'static str,
    content: HashMap<String, Rc<Vec<Entry>>>,
    portal_status: HashMap<String, &'static str>,
    portal_roots: HashMap<String, Vec<Entry>>,
    loose_entries: HashMap<String, Entry>,
    folders: HashMap<String, Vec<PathBuf>>,
    hidden: bool,
    peek: bool,
    next_desktop_check: Instant,
    next_reroute: Instant,
}

fn load(path: &Path) -> (Layout, bool) {
    match layout::read(path) {
        Ok(layout) => (layout, true),
        Err(_) => match layout::read(&layout::sidecar(path, ".bak")) {
            Ok(layout) => (layout, false),
            Err(_) => (
                Layout::initial(work_area(None, None), dpi(HWND::default())),
                false,
            ),
        },
    }
}

impl App {
    fn new(hwnd: HWND, data: &Path, signal: Handle) -> Result<Self, Box<dyn std::error::Error>> {
        let path = data.join("layout.xml");
        let (layout, has_valid_primary) = load(&path);
        let mut desktop_folders = Vec::new();
        for id in [FOLDERID_Desktop, FOLDERID_PublicDesktop] {
            if let Ok(folder) = known_folder(&id)
                && !desktop_folders
                    .iter()
                    .any(|path: &PathBuf| path_key(path) == path_key(&folder))
            {
                desktop_folders.push(folder);
            }
        }
        if let Some(test_desktop) = std::env::var_os("DESKGRID_DESKTOP_DIR") {
            desktop_folders = vec![std::path::absolute(test_desktop)?];
        }
        crate::i18n::set(layout.language);
        Ok(Self {
            hwnd,
            signal,
            tray_icon: None,
            explorer: explorer(),
            desktop_input: gesture::DesktopInput::new(hwnd).ok(),
            native_icons: None,
            icons: IconGuard::new(data)?,
            icon_recovery_pending: true,
            reported_icon_error: false,
            path,
            has_valid_primary,
            preserved_invalid_primary: false,
            save_pending: false,
            reported_save_error: false,
            next_save_retry: Instant::now(),
            layout,
            windows: HashMap::new(),
            loose: HashMap::new(),
            scanner: Scanner::new()?,
            desktop_folders,
            desktop_keys: HashSet::new(),
            shell_items: Vec::new(),
            desktop_ready: false,
            desktop_status: tr!("正在读取桌面…", "正在讀取桌面…", "Loading desktop…"),
            content: HashMap::new(),
            portal_status: HashMap::new(),
            portal_roots: HashMap::new(),
            loose_entries: HashMap::new(),
            folders: HashMap::new(),
            hidden: false,
            peek: false,
            next_desktop_check: Instant::now(),
            next_reroute: Instant::now(),
        })
    }

    fn tray(&mut self) {
        let icon = self.tray_icon.get_or_insert_with(|| {
            let mut data = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: TRAY_MESSAGE,
                hIcon: resource_icon(),
                ..Default::default()
            };
            let tooltip = wide(tr!("桌面格 · DeskGrid", "桌面格 · DeskGrid", "DeskGrid"));
            data.szTip[..tooltip.len()].copy_from_slice(&tooltip);
            Tray(data)
        });
        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &icon.0);
        }
    }

    fn notify(&self, message: &str) {
        if let Some(tray) = &self.tray_icon {
            let mut data = tray.0;
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = NIIF_WARNING;
            let mut title = wide(tr!("桌面格", "桌面格", "DeskGrid"));
            title.truncate(data.szInfoTitle.len() - 1);
            data.szInfoTitle[..title.len()].copy_from_slice(&title);
            let mut body = wide(message);
            body.truncate(data.szInfo.len() - 1);
            data.szInfo[..body.len()].copy_from_slice(&body);
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
            }
        }
    }

    fn save(&mut self) {
        self.save_pending = true;
        let result = (|| {
            if !self.has_valid_primary
                && !self.preserved_invalid_primary
                && self.path.try_exists()?
            {
                let preserved =
                    layout::sidecar(&self.path, &format!(".invalid-{}", layout::new_id()));
                layout::copy_atomic(&self.path, &preserved)?;
                self.preserved_invalid_primary = true;
            }
            layout::write(&self.path, &self.layout, self.has_valid_primary)
        })();
        match result {
            Ok(()) => {
                self.has_valid_primary = true;
                self.save_pending = false;
                self.reported_save_error = false;
            }
            Err(error) => {
                self.next_save_retry = Instant::now() + Duration::from_secs(5);
                if !self.reported_save_error {
                    self.notify(&tr_format!(
                        "布局保存失败，将重试。{error}",
                        "無法儲存版面，將重試。{error}",
                        "Could not save the layout. DeskGrid will retry. {error}"
                    ));
                    self.reported_save_error = true;
                }
            }
        }
    }

    fn changed(&mut self) {
        self.save();
        self.tick(true);
        dashboard::refresh(dashboard::window(), &self.layout);
    }

    fn owner(&self) -> HWND {
        let manager = dashboard::window();
        if unsafe { IsWindowVisible(manager) }.as_bool() {
            manager
        } else {
            self.hwnd
        }
    }

    fn folder(&self, group: &Group) -> Option<PathBuf> {
        self.folders
            .get(&group.id)
            .and_then(|stack| stack.last())
            .or(group.folder.as_ref())
            .cloned()
    }

    fn tick(&mut self, force: bool) {
        let now = Instant::now();
        if unsafe { WaitForSingleObject(self.signal.0, 0) } == WAIT_OBJECT_0 {
            dispatch(Action::Manage);
        }
        if self.save_pending && now >= self.next_save_retry {
            self.save();
        }
        let check = force || now >= self.next_desktop_check;
        let mut reroute = force || now >= self.next_reroute;
        if check {
            self.next_desktop_check = now + Duration::from_secs(1);
            if self.icon_recovery_pending
                && restore_legacy_icons(self.path.parent().unwrap()).is_ok()
            {
                self.icon_recovery_pending = false;
            }
            let explorer = explorer();
            if self.explorer != explorer {
                self.native_icons = None;
                self.shell_items.clear();
                reroute = true;
            }
            self.explorer = explorer;
            if explorer.icons_hidden() {
                self.native_icons = None;
            } else if let Some(icons) = explorer.icons()
                && self.native_icons.as_deref() != Some(&icons)
            {
                self.native_icons = Some(icons.into());
            }
            if let Some(input) = &self.desktop_input {
                input.update(
                    explorer,
                    self.native_icons.clone(),
                    self.layout.quick_hide_on_double_click,
                );
            }
        }
        if reroute
            && std::env::var_os("DESKGRID_DESKTOP_DIR").is_none()
            && let Ok(items) = desktop_system_items()
        {
            self.shell_items = items;
        }
        if self.scan(reroute) || check {
            self.sync_panels();
        }
        self.sync_icons();
    }

    fn sync_icons(&mut self) {
        if self.icon_recovery_pending || std::env::var_os("DESKGRID_DESKTOP_DIR").is_some() {
            return;
        }
        let panels_available = !self.windows.is_empty()
            && (self.hidden || self.windows.values().copied().any(panel::is_shown));
        let hide = self.layout.hide_windows_icons && self.desktop_ready && panels_available;
        if let Err(error) = self.icons.update(self.explorer.list, hide) {
            if !self.reported_icon_error {
                self.notify(&tr_format!(
                    "无法切换 Windows 桌面图标。{error}",
                    "無法切換 Windows 桌面圖示。{error}",
                    "Could not change Windows desktop icon visibility. {error}"
                ));
            }
            self.reported_icon_error = true;
        } else {
            self.reported_icon_error = false;
        }
    }

    fn scan(&mut self, force: bool) -> bool {
        let changed = self.scanner.poll();
        let desktop: Vec<_> = self
            .desktop_folders
            .iter()
            .map(|path| self.scanner.add(path, false, true))
            .collect();
        let pins: Vec<_> = self
            .layout
            .placements
            .items
            .iter()
            .filter(|p| p.pinned)
            .filter(|p| !self.desktop_keys.contains(&path_key(&p.path)))
            .map(|p| self.scanner.add(&p.path, true, false))
            .collect();
        let mut portals = Vec::new();
        for group in &self.layout.groups.items {
            if let (Some(root), Some(path)) = (&group.folder, self.folder(group)) {
                let source = self.scanner.add(&path, false, false);
                let home = if path_key(root) != path_key(&path) {
                    Some(self.scanner.add(root, false, false))
                } else {
                    None
                };
                portals.push((group, source, home));
            }
        }
        self.scanner.prune();
        if !changed && !force {
            return false;
        }
        self.next_reroute = Instant::now() + Duration::from_secs(30);
        let from_desktop: Vec<_> = desktop
            .iter()
            .flat_map(|source| self.scanner.get(source).entries())
            .chain(&self.shell_items)
            .cloned()
            .collect();
        self.desktop_keys = from_desktop.iter().map(|entry| entry.key.clone()).collect();
        self.desktop_ready = desktop
            .iter()
            .any(|source| matches!(self.scanner.get(source), Listing::Available(_)));
        self.desktop_status = if self.desktop_ready {
            ""
        } else if desktop
            .iter()
            .any(|source| self.scanner.get(source) == &Listing::Loading)
        {
            tr!("正在读取桌面…", "正在讀取桌面…", "Loading desktop…")
        } else {
            tr!(
                "桌面目录不可用",
                "桌面資料夾無法使用",
                "Desktop folder unavailable"
            )
        };
        let all = from_desktop.into_iter().chain(
            pins.iter()
                .flat_map(|source| self.scanner.get(source).entries())
                .cloned(),
        );
        let loose: HashSet<_> = self
            .layout
            .placements
            .items
            .iter()
            .filter(|p| p.loose)
            .map(|p| path_key(&p.path))
            .collect();
        self.loose_entries = all
            .clone()
            .filter(|entry| loose.contains(&entry.key))
            .map(|entry| (entry.key.clone(), entry))
            .collect();
        self.content = self
            .layout
            .route(all, SystemTime::now())
            .into_iter()
            .map(|(id, entries)| (id, Rc::new(entries)))
            .collect();
        self.portal_status.clear();
        self.portal_roots.clear();
        for (group, source, home) in portals {
            let listing = self.scanner.get(&source);
            let status = match listing {
                Listing::Available(_) => "",
                Listing::Loading => tr!("正在读取文件夹…", "正在讀取資料夾…", "Loading folder…"),
                Listing::Unavailable => tr!("文件夹不可用", "資料夾無法使用", "Folder unavailable"),
            };
            let mut entries = listing.entries().to_vec();
            group.sort_items(&mut entries);
            self.content.insert(group.id.clone(), Rc::new(entries));
            self.portal_status.insert(group.id.clone(), status);
            if let Some(home) = home {
                self.portal_roots
                    .insert(group.id.clone(), self.scanner.get(&home).entries().to_vec());
            }
        }
        true
    }

    fn fitted(&self, placed: Bounds, minimum: (i32, i32)) -> Bounds {
        placed.fit(work_area(Some(placed), None), minimum)
    }

    fn sync_panels(&mut self) {
        let visible = self.peek || (!self.hidden && !self.explorer.host.is_invalid());
        let parent = if self.peek {
            HWND::default()
        } else {
            self.explorer.host
        };
        let active_page = &self.layout.active_page_id;
        let roots: Vec<_> = self
            .layout
            .groups
            .items
            .iter()
            .filter(|group| {
                group.tab_host_id.is_none() && group.page_ids.items.contains(active_page)
            })
            .map(|group| (group, self.fitted(group.bounds(), (192, 125))))
            .collect();
        let mut specs = Vec::new();
        for (root, placed) in &roots {
            let active = root
                .active_tab_id
                .as_deref()
                .and_then(|id| self.layout.group(id))
                .unwrap_or(root);
            let folder = self.folder(active);
            let status = if folder.is_some() {
                self.portal_status.get(&active.id).copied().unwrap_or("")
            } else {
                self.desktop_status
            };
            specs.push((
                root.id.clone(),
                panel::Spec {
                    frame: (*root).clone(),
                    active: active.clone(),
                    tabs: self.layout.tabs(root).cloned().collect(),
                    folder,
                    entries: self.content.get(&active.id).cloned().unwrap_or_default(),
                    status,
                    placed: *placed,
                    neighbors: Rc::new(
                        roots
                            .iter()
                            .filter(|(group, _)| group.id != root.id)
                            .map(|(_, bounds)| *bounds)
                            .collect(),
                    ),
                    loose: false,
                },
            ));
        }
        let mut tiles = Vec::new();
        for placement in self.layout.placements.items.iter().filter(|p| p.loose) {
            let key = path_key(&placement.path);
            let Some(entry) = self.loose_entries.get(&key) else {
                continue;
            };
            let scale = dpi(HWND::default());
            let placed = self.fitted(
                Bounds {
                    left: placement.left,
                    top: placement.top,
                    width: px(84, scale),
                    height: px(78, scale),
                },
                (1, 1),
            );
            let mut frame = Group::new(&entry.label, 0, placed, active_page);
            frame.id.clone_from(&key);
            tiles.push((
                key,
                panel::Spec {
                    active: frame.clone(),
                    frame,
                    tabs: Vec::new(),
                    folder: None,
                    entries: Rc::new(vec![entry.clone()]),
                    status: "",
                    placed,
                    neighbors: Rc::new(Vec::new()),
                    loose: true,
                },
            ));
        }
        sync_windows(&mut self.windows, specs, parent, self.peek, visible);
        sync_windows(&mut self.loose, tiles, parent, self.peek, visible);
    }

    fn handle(&mut self, action: Action) -> bool {
        match action {
            Action::Exit => return false,
            Action::Tick => {
                TICK_PENDING.with(|pending| pending.set(false));
                self.tick(false);
            }
            Action::Refresh => {
                self.scanner.refresh();
                self.tick(true);
            }
            Action::RestoreTray => {
                self.tray();
                self.tick(true);
            }
            Action::Manage => self.manage(),
            Action::TrayMenu => self.tray_menu(),
            Action::PanelMenu(hwnd, root, id, point, item) => {
                self.panel_menu(hwnd, &root, &id, point, item)
            }
            Action::Hidden => {
                self.hidden = !self.hidden;
                self.peek = false;
                self.tick(true);
            }
            Action::Peek => {
                self.peek = !self.peek;
                self.tick(true);
            }
            Action::Search(id) => self.search(id.as_deref()),
            Action::StepPage(step) => {
                let pages = &self.layout.pages.items;
                if let Some(i) = pages
                    .iter()
                    .position(|page| page.id == self.layout.active_page_id)
                {
                    let next = (i as isize + step).rem_euclid(pages.len() as isize) as usize;
                    self.switch_page(pages[next].id.clone());
                }
            }
            Action::SwitchPage(id) => self.switch_page(id),
            Action::DrawGroup(bounds) => {
                self.layout.draw_group(bounds);
                self.hidden = false;
                self.peek = false;
                self.changed();
            }
            Action::NewPage => {
                self.new_page();
            }
            Action::Place(id, bounds) => {
                if let Some(group) = self.layout.group_mut(&id) {
                    group.set_bounds(bounds);
                    self.changed();
                }
            }
            Action::Collapse(id) => {
                if let Some(group) = self.layout.group_mut(&id) {
                    group.collapsed = !group.collapsed;
                    self.changed();
                }
            }
            Action::Activate(id) => {
                self.layout.activate(&id);
                self.changed();
            }
            Action::Navigate(id, path) => self.navigate(&id, path),
            Action::Back(id) => {
                if let Some(stack) = self.folders.get_mut(&id) {
                    stack.pop();
                    self.tick(true);
                }
            }
            Action::Open(path) => self.open(&path),
            Action::Reveal(path) => {
                if let Err(error) = reveal(&path) {
                    self.notify(&tr_format!(
                        "无法在资源管理器中定位。{error}",
                        "無法在檔案總管中定位。{error}",
                        "Could not show the item in File Explorer. {error}"
                    ));
                }
            }
            Action::Copy(path) => {
                if let Err(error) = copy_to_clipboard(self.hwnd, &path) {
                    self.notify(&tr_format!(
                        "无法复制路径。{error}",
                        "無法複製路徑。{error}",
                        "Could not copy the path. {error}"
                    ));
                }
            }
            Action::Drag(hwnd, entry) => self.drag(hwnd, entry),
            Action::Assign(id, paths, display, slot) => {
                self.layout
                    .assign(&id, paths, &self.desktop_keys, &display, slot);
                self.layout.activate(&id);
                self.changed();
            }
            Action::Clear(path) => {
                let key = path_key(&path);
                self.layout
                    .placements
                    .items
                    .retain(|p| path_key(&p.path) != key);
                self.changed();
            }
            Action::EditGroup(id) => self.edit_group(Some(&id), false),
            Action::NewGroup(portal) => self.edit_group(None, portal),
            Action::RemoveGroup(id) => self.remove_group(&id),
            Action::DefaultGroup(id) => {
                if self
                    .layout
                    .group(&id)
                    .is_some_and(|group| group.folder.is_none())
                {
                    self.layout.default_group_id = id;
                    self.changed();
                }
            }
            Action::Membership(id) => self.membership(&id),
            Action::Dock(id) => self.dock(&id),
            Action::Detach(id) => {
                let position = self.new_bounds();
                self.layout.detach(&id);
                if let Some(group) = self.layout.group_mut(&id) {
                    group.set_bounds(position);
                }
                self.changed();
            }
            Action::Pages => self.pages(),
            Action::Rules => self.rules(),
            Action::Align => self.align(),
            Action::Options => self.options(),
            Action::Import => self.import(),
            Action::Export => self.export(),
        }
        true
    }

    fn switch_page(&mut self, id: String) {
        if self.layout.active_page_id != id
            && self.layout.pages.items.iter().any(|page| page.id == id)
        {
            self.layout.active_page_id = id;
            self.changed();
        }
    }

    fn navigate(&mut self, id: &str, path: PathBuf) {
        if self
            .layout
            .group(id)
            .is_some_and(|group| group.folder.is_some())
        {
            self.folders.entry(id.into()).or_default().push(path);
            self.tick(true);
        }
    }

    fn open(&self, path: &Path) {
        if let Err(error) = platform::open(self.hwnd, path) {
            self.notify(&tr_format!(
                "无法打开 {}。{error}",
                "無法開啟 {}。{error}",
                "Could not open {}. {error}",
                path.display()
            ));
        }
    }

    fn drag(&mut self, hwnd: HWND, entry: Entry) {
        if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } >= 0 {
            return;
        }
        match drag::start(hwnd, &entry) {
            Ok((DROPEFFECT_NONE, Some(point)))
                if self.layout.hide_windows_icons
                    && self.explorer.icons_hidden()
                    && !self.peek
                    && self.explorer.blank(point, self.native_icons.as_deref()) =>
            {
                let Some(path) = full_path(&entry.path) else {
                    return;
                };
                let pinned = !self.desktop_keys.contains(&entry.key);
                self.layout
                    .placements
                    .items
                    .retain(|p| path_key(&p.path) != entry.key);
                self.layout.placements.items.push(Placement {
                    path,
                    pinned,
                    loose: true,
                    left: point.x - px(42, dpi(hwnd)),
                    top: point.y - px(16, dpi(hwnd)),
                    ..Placement::default()
                });
                self.changed();
            }
            Ok(_) => {
                self.scanner.refresh();
            }
            Err(error) => self.notify(&tr_format!(
                "无法拖动 {}。{error}",
                "無法拖曳 {}。{error}",
                "Could not drag {}. {error}",
                entry.label
            )),
        }
    }

    fn search(&mut self, scope: Option<&str>) {
        self.tick(true);
        let mut rows = Vec::new();
        for group in self
            .layout
            .groups
            .items
            .iter()
            .filter(|group| scope.is_none_or(|id| group.id == id))
        {
            let group_id = Some(group.id.clone());
            let folder = self.folder(group);
            let description = group.title.clone();
            if let Some(entries) = self.content.get(&group.id) {
                rows.extend(entries.iter().cloned().map(|entry| SearchRow {
                    group: group_id.clone(),
                    folder: folder.clone(),
                    entry,
                    description: description.clone(),
                }));
            }
            if let Some(entries) = self.portal_roots.get(&group.id) {
                rows.extend(entries.iter().cloned().map(|entry| SearchRow {
                    group: group_id.clone(),
                    folder: group.folder.clone(),
                    entry,
                    description: description.clone(),
                }));
            }
        }
        if scope.is_none() {
            rows.extend(self.loose_entries.values().cloned().map(|entry| SearchRow {
                group: None,
                folder: None,
                entry,
                description: tr!("桌面独立图标", "桌面獨立圖示", "Standalone desktop icon").into(),
            }));
        }
        rows.sort_by_cached_key(|row| (row.entry.label.to_lowercase(), row.description.clone()));
        let titles: Vec<_> = rows
            .iter()
            .map(|row| (row.entry.label.clone(), row.description.clone()))
            .collect();
        if let Some((index, locate)) = dialog::search(self.owner(), &titles) {
            let row = &rows[index];
            if !locate {
                self.open(&row.entry.path);
                return;
            }
            self.hidden = false;
            self.peek = true;
            if let Some(id) = &row.group
                && let Some(group) = self.layout.group(id)
            {
                let root = self.layout.root(group);
                let root_id = root.id.clone();
                let reset_folder = self.folder(group) != row.folder && row.folder == group.folder;
                if !root.page_ids.items.contains(&self.layout.active_page_id) {
                    self.layout.active_page_id = root.page_ids.items[0].clone();
                }
                if reset_folder {
                    self.folders.remove(id);
                }
                self.layout.activate(id);
                self.changed();
                if let Some(&window) = self.windows.get(&root_id) {
                    panel::locate(window, row.entry.key.clone());
                }
            } else {
                self.tick(true);
                if let Some(&window) = self.loose.get(&row.entry.key) {
                    panel::locate(window, row.entry.key.clone());
                }
            }
        }
    }
}

fn sync_windows(
    windows: &mut HashMap<String, HWND>,
    specs: Vec<(String, panel::Spec)>,
    parent: HWND,
    peek: bool,
    visible: bool,
) {
    let seen: HashSet<_> = specs.iter().map(|(key, _)| key.as_str()).collect();
    windows.retain(|key, hwnd| {
        let matching = panel::lookup(*hwnd).is_some_and(|state| {
            let state = state.borrow();
            state.parent == parent && state.peek == peek
        });
        if seen.contains(key.as_str()) && matching {
            return true;
        }
        if matching || panel::lookup(*hwnd).is_some() {
            unsafe {
                let _ = DestroyWindow(*hwnd);
            }
        }
        false
    });
    for (key, spec) in specs {
        let hwnd = if let Some(&hwnd) = windows.get(&key) {
            hwnd
        } else {
            if !visible {
                continue;
            }
            let Ok(hwnd) = panel::create(parent, peek, spec.clone()) else {
                continue;
            };
            windows.insert(key, hwnd);
            hwnd
        };
        panel::update(hwnd, spec);
        panel::show(hwnd, visible);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if self.save_pending {
            self.save();
        }
        self.desktop_input = None;
        for &hwnd in self.windows.values().chain(self.loose.values()) {
            if panel::lookup(hwnd).is_some() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
        self.tray_icon = None;
        unsafe {
            let manager = dashboard::window();
            if !manager.is_invalid() {
                let _ = DestroyWindow(manager);
            }
            let _ = KillTimer(Some(self.hwnd), 1);
            for id in 1..=5 {
                let _ = UnregisterHotKey(Some(self.hwnd), id);
            }
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn copy_to_clipboard(owner: HWND, path: &Path) -> windows::core::Result<()> {
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;

    let value = wide(path);
    unsafe {
        OpenClipboard(Some(owner))?;
        let result = (|| {
            EmptyClipboard()?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, value.len() * 2)?;
            let pointer = GlobalLock(memory);
            if pointer.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(windows::core::Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(value.as_ptr(), pointer.cast(), value.len());
            let _ = GlobalUnlock(memory);
            let result =
                SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0))).map(|_| ());
            if result.is_err() {
                let _ = GlobalFree(Some(memory));
            }
            result
        })();
        let _ = CloseClipboard();
        result
    }
}
