use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use crate::directory::Entry;
use crate::layout::{Bounds, Group, View};

use super::platform::*;
use super::{Action, dispatch};

pub const CLASS: PCWSTR = w!("DeskGrid.Panel");
pub const COLORS: [COLORREF; 8] = [
    rgb(109, 179, 255),
    rgb(92, 217, 171),
    rgb(239, 174, 108),
    rgb(189, 149, 255),
    rgb(255, 137, 162),
    rgb(102, 213, 229),
    rgb(233, 209, 117),
    rgb(172, 187, 209),
];
const BG: COLORREF = rgb(23, 30, 41);
const HEADER: COLORREF = rgb(32, 42, 57);
const INK: COLORREF = rgb(244, 247, 252);
const MUTED: COLORREF = rgb(179, 193, 214);
const SELECTED: COLORREF = rgb(49, 65, 87);
const KEY: COLORREF = rgb(1, 2, 3);

const HOVER_TIMER: usize = 1;
const SCROLL_TIMER: usize = 2;

const fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | g << 8 | b << 16)
}

#[derive(Clone)]
pub struct Spec {
    pub frame: Group,
    pub active: Group,
    pub tabs: Vec<Group>,
    pub folder: Option<PathBuf>,
    pub entries: Rc<Vec<Entry>>,
    pub status: &'static str,
    pub placed: Bounds,
    pub neighbors: Rc<Vec<Bounds>>,
    pub loose: bool,
}

pub enum DropPlan {
    Shell(PathBuf),
    Assign {
        group: String,
        slot: Option<usize>,
        entries: Rc<Vec<Entry>>,
        mark: Option<RECT>,
    },
}

#[derive(Clone, Copy)]
enum Gesture {
    Move(POINT, Bounds),
    Resize(POINT, Bounds),
    Item(POINT),
}

pub struct Panel {
    pub hwnd: HWND,
    pub parent: HWND,
    pub peek: bool,
    spec: Spec,
    selected: Option<String>,
    scroll: usize,
    tab_start: usize,
    hover: bool,
    expanded: bool,
    gesture: Option<Gesture>,
    drop_mark: Option<RECT>,
    drop_point: Option<POINT>,
}

thread_local! {
    static PANELS: RefCell<HashMap<isize, Rc<RefCell<Panel>>>> = RefCell::new(HashMap::new());
}

pub fn lookup(hwnd: HWND) -> Option<Rc<RefCell<Panel>>> {
    PANELS.with(|panels| panels.borrow().get(&(hwnd.0 as isize)).cloned())
}

pub fn create(parent: HWND, peek: bool, spec: Spec) -> Result<HWND> {
    create_window(parent, peek, spec, false)
}

fn create_window(parent: HWND, peek: bool, spec: Spec, preview: bool) -> Result<HWND> {
    let parent = (!parent.is_invalid()).then_some(parent);
    let title = wide(&spec.active.title);
    let style = if parent.is_some() {
        WS_CHILD | WS_CLIPSIBLINGS
    } else {
        WS_POPUP
    };
    let mut ex_style = WS_EX_TOOLWINDOW | WS_EX_LAYERED;
    if peek {
        ex_style |= WS_EX_TOPMOST;
    }
    if preview {
        ex_style |= WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;
    }
    let hwnd = unsafe {
        let previous =
            parent.map(|parent| SetThreadDpiAwarenessContext(GetWindowDpiAwarenessContext(parent)));
        let window = CreateWindowExW(
            ex_style,
            CLASS,
            PCWSTR(title.as_ptr()),
            style,
            0,
            0,
            0,
            0,
            parent,
            None,
            Some(instance()),
            None,
        );
        if let Some(previous) = previous.filter(|value| !value.0.is_null()) {
            SetThreadDpiAwarenessContext(previous);
        }
        window?
    };
    PANELS.with(|panels| {
        panels.borrow_mut().insert(
            hwnd.0 as isize,
            Rc::new(RefCell::new(Panel {
                hwnd,
                parent: parent.unwrap_or_default(),
                peek,
                spec,
                selected: None,
                scroll: 0,
                tab_start: 0,
                hover: false,
                expanded: false,
                gesture: None,
                drop_mark: None,
                drop_point: None,
            })),
        )
    });
    if !preview && let Err(error) = super::drag::register(hwnd) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    }
    Ok(hwnd)
}

pub struct Preview(HWND);

impl Preview {
    pub fn new(entry: &Entry, source: HWND) -> Option<Self> {
        let scale = dpi(source);
        let point = cursor();
        let placed = Bounds {
            left: point.x + 16,
            top: point.y + 18,
            width: px(92, scale),
            height: px(76, scale),
        };
        let frame = Group {
            title: entry.label.clone(),
            ..Group::default()
        };
        let spec = Spec {
            active: frame.clone(),
            frame,
            tabs: Vec::new(),
            folder: None,
            entries: Rc::new(vec![entry.clone()]),
            status: "",
            placed,
            neighbors: Rc::new(Vec::new()),
            loose: true,
        };
        let hwnd = create_window(HWND::default(), true, spec.clone(), true).ok()?;
        update(hwnd, spec);
        show(hwnd, true);
        Some(Self(hwnd))
    }

    pub fn follow_pointer(&self) {
        let point = cursor();
        unsafe {
            let _ = SetWindowPos(
                self.0,
                None,
                point.x + 16,
                point.y + 18,
                0,
                0,
                SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

pub fn update(hwnd: HWND, spec: Spec) {
    let Some(panel) = lookup(hwnd) else { return };
    let mut panel = panel.borrow_mut();
    if panel.spec.active.id != spec.active.id || panel.spec.folder != spec.folder {
        panel.selected = None;
        panel.scroll = 0;
    }
    if panel.spec.frame.collapsed != spec.frame.collapsed {
        panel.expanded = false;
    }
    if let Some(index) = spec
        .tabs
        .iter()
        .position(|group| group.id == spec.active.id)
    {
        let max_tabs = (spec.placed.width / panel.scale(76)).max(1) as usize;
        if index < panel.tab_start || index >= panel.tab_start + max_tabs {
            panel.tab_start = index.saturating_sub(max_tabs - 1);
        }
    }
    panel.spec = spec;
    if panel.gesture.is_none() {
        panel.position(panel.spec.placed, false);
    }
    panel.opacity();
    let grid = panel.grid();
    panel.scroll = panel.scroll.min(grid.limit(panel.spec.entries.len()));
    invalidate(hwnd);
}

pub fn show(hwnd: HWND, visible: bool) {
    let pane_visible = visible_style(hwnd);
    unsafe {
        if visible && (!pane_visible || covered_by_desktop_layer(hwnd)) {
            let topmost = (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0) != 0;
            // Explorer can create or reorder child windows below the wallpaper and icon layers.
            // Keep the panel above those siblings without changing focus or the order of other panels.
            if SetWindowPos(
                hwnd,
                Some(if topmost { HWND_TOPMOST } else { HWND_TOP }),
                0,
                0,
                0,
                0,
                SWP_SHOWWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )
            .is_ok()
            {
                invalidate(hwnd);
            }
        } else if !visible && pane_visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

fn visible_style(hwnd: HWND) -> bool {
    (unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32 & WS_VISIBLE.0) != 0
}

pub fn is_shown(hwnd: HWND) -> bool {
    (unsafe { IsWindowVisible(hwnd) }).as_bool() && !covered_by_desktop_layer(hwnd)
}

fn covered_by_desktop_layer(hwnd: HWND) -> bool {
    if lookup(hwnd).is_none_or(|panel| panel.borrow().parent.is_invalid()) {
        return false;
    }
    let mut current = hwnd;
    while let Ok(above) = unsafe { GetWindow(current, GW_HWNDPREV) } {
        if lookup(above).is_none() && visible_style(above) {
            return true;
        }
        current = above;
    }
    false
}

pub fn locate(hwnd: HWND, key: String) {
    if let Some(state) = lookup(hwnd) {
        let mut panel = state.borrow_mut();
        let mut position = None;
        if let Some(index) = panel.spec.entries.iter().position(|entry| entry.key == key) {
            panel.selected = Some(key);
            let grid = panel.grid();
            let row = index / grid.columns;
            if row < panel.scroll || row >= panel.scroll + grid.rows {
                position = Some(row.min(grid.limit(panel.spec.entries.len())));
            }
        }
        if let Some(scroll) = position {
            panel.scroll = scroll;
        }
        invalidate(hwnd);
    }
}

fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

struct Grid {
    zone: RECT,
    width: i32,
    height: i32,
    columns: usize,
    rows: usize,
    first: usize,
}

impl Grid {
    fn limit(&self, count: usize) -> usize {
        count.div_ceil(self.columns).saturating_sub(self.rows)
    }

    fn cell(&self, index: usize) -> RECT {
        let col = (index % self.columns) as i32;
        let row = (index / self.columns) as i32 - self.first as i32;
        rect(
            self.zone.left + col * self.width,
            self.zone.top + row * self.height,
            self.width,
            self.height,
        )
    }

    fn hit(&self, point: POINT) -> Option<usize> {
        if !contains(self.zone, point) {
            return None;
        }
        let col = ((point.x - self.zone.left) / self.width) as usize;
        let row = ((point.y - self.zone.top) / self.height) as usize;
        (col < self.columns).then_some((row + self.first) * self.columns + col)
    }
}

impl Panel {
    fn scale(&self, value: i32) -> i32 {
        px(value, dpi(self.hwnd))
    }

    fn collapsed(&self) -> bool {
        self.spec.frame.collapsed && !self.expanded
    }

    fn position(&self, placed: Bounds, raise: bool) {
        let mut origin = POINT {
            x: placed.left,
            y: placed.top,
        };
        let height = if self.collapsed() {
            self.scale(36)
        } else {
            placed.height
        };
        unsafe {
            if !self.parent.is_invalid() {
                let _ = ScreenToClient(self.parent, &mut origin);
            }
            let flags = SWP_NOACTIVATE
                | SWP_NOOWNERZORDER
                | if raise {
                    SET_WINDOW_POS_FLAGS(0)
                } else {
                    SWP_NOZORDER
                };
            let _ = SetWindowPos(
                self.hwnd,
                Some(if self.peek { HWND_TOPMOST } else { HWND_TOP }),
                origin.x,
                origin.y,
                placed.width,
                height,
                flags,
            );
        }
    }

    fn opacity(&self) {
        let percent = if self.spec.active.dim_when_idle && !self.hover {
            self.spec.active.opacity.min(55)
        } else {
            self.spec.active.opacity
        };
        unsafe {
            let _ = if self.spec.loose {
                SetLayeredWindowAttributes(self.hwnd, KEY, 255, LWA_COLORKEY)
            } else {
                SetLayeredWindowAttributes(
                    self.hwnd,
                    COLORREF(0),
                    (percent * 255 / 100) as u8,
                    LWA_ALPHA,
                )
            };
        }
    }

    fn tab_rects(&self) -> impl Iterator<Item = (&Group, RECT)> {
        let edge = self.scale(9);
        let width = client(self.hwnd).right - edge * 2;
        let maximum = (width / self.scale(76)).max(1) as usize;
        let tab_width =
            (width / maximum.min(self.spec.tabs.len()).max(1) as i32).min(self.scale(150));
        self.spec
            .tabs
            .iter()
            .skip(self.tab_start)
            .take(maximum)
            .filter(|_| self.spec.tabs.len() > 1 && !self.collapsed())
            .enumerate()
            .map(move |(i, tab)| {
                (
                    tab,
                    rect(
                        edge + tab_width * i as i32,
                        self.scale(38),
                        tab_width,
                        self.scale(26),
                    ),
                )
            })
    }

    fn portal_bar(&self) -> Option<RECT> {
        self.spec.folder.as_ref().map(|_| {
            let y = if self.spec.tabs.len() > 1 { 68 } else { 39 };
            rect(
                self.scale(9),
                self.scale(y),
                client(self.hwnd).right - self.scale(18),
                self.scale(25),
            )
        })
    }

    fn grid(&self) -> Grid {
        let client = client(self.hwnd);
        let x = self.scale(if self.spec.loose { 0 } else { 9 });
        let mut top = if self.spec.loose {
            0
        } else if self.spec.tabs.len() > 1 {
            self.scale(69)
        } else {
            self.scale(40)
        };
        if let Some(portal) = self.portal_bar() {
            top = portal.bottom + self.scale(5);
        }
        if self.spec.active.view == View::Details {
            top += self.scale(22);
        }
        let available = (client.right - 2 * x).max(1);
        let (columns, height) = match self.spec.active.view {
            _ if self.spec.loose => (1, client.bottom.max(1)),
            View::Icons => (
                (available / self.scale(self.spec.active.icon_size + 50)).max(1),
                self.scale(self.spec.active.icon_size + 40),
            ),
            _ => (1, self.scale(self.spec.active.icon_size + 8)),
        };
        let bottom = if self.collapsed() {
            top
        } else {
            (client.bottom - x).max(top)
        };
        Grid {
            zone: rect(x, top, available, bottom - top),
            width: (available / columns).max(1),
            height,
            columns: columns as usize,
            rows: ((bottom - top) / height).max(1) as usize,
            first: self.scroll,
        }
    }

    fn menu_rect(&self) -> RECT {
        rect(
            client(self.hwnd).right - self.scale(68),
            self.scale(3),
            self.scale(32),
            self.scale(30),
        )
    }

    fn collapse_rect(&self) -> RECT {
        rect(
            client(self.hwnd).right - self.scale(35),
            self.scale(3),
            self.scale(31),
            self.scale(30),
        )
    }

    fn over_grip(&self, point: POINT) -> bool {
        let area = client(self.hwnd);
        !self.spec.loose
            && !self.collapsed()
            && !self.spec.frame.locked
            && point.x >= area.right - self.scale(16)
            && point.y >= area.bottom - self.scale(16)
    }

    fn entry(&self, point: POINT) -> Option<&Entry> {
        self.grid()
            .hit(point)
            .and_then(|index| self.spec.entries.get(index))
    }

    fn selected(&self) -> Option<&Entry> {
        self.spec
            .entries
            .iter()
            .find(|entry| Some(&entry.key) == self.selected.as_ref())
    }

    fn open(&self, entry: &Entry) {
        if self.spec.folder.is_some() && entry.folder {
            dispatch(Action::Navigate(
                self.spec.active.id.clone(),
                entry.path.clone(),
            ));
        } else {
            dispatch(Action::Open(entry.path.clone()));
        }
    }

    fn menu(&self, point: POINT, item: Option<Entry>) {
        dispatch(Action::PanelMenu(
            self.hwnd,
            self.spec.frame.id.clone(),
            self.spec.active.id.clone(),
            point,
            item,
        ));
    }

    fn button_down(&mut self, point: POINT, double: bool) {
        if !self.spec.loose {
            if contains(self.menu_rect(), point) {
                if !double {
                    self.menu(cursor(), None);
                }
                return;
            }
            if contains(self.collapse_rect(), point) || (double && point.y < self.scale(36)) {
                if !double || !contains(self.collapse_rect(), point) {
                    self.cancel_gesture();
                    dispatch(Action::Collapse(self.spec.frame.id.clone()));
                }
                return;
            }
            if let Some((tab, _)) = self.tab_rects().find(|(_, area)| contains(*area, point)) {
                if !double {
                    dispatch(Action::Activate(tab.id.clone()));
                }
                return;
            }
            if !self.collapsed() && self.portal_bar().is_some_and(|area| contains(area, point)) {
                if !double {
                    if point.x < self.scale(43) {
                        dispatch(Action::Back(self.spec.active.id.clone()));
                    } else {
                        dispatch(Action::Open(self.spec.folder.clone().unwrap()));
                    }
                }
                return;
            }
        }
        let screen = cursor();
        if self.over_grip(point) {
            self.gesture = Some(Gesture::Resize(screen, self.spec.placed));
        } else if !self.spec.loose && point.y < self.scale(36) && !self.spec.frame.locked {
            self.gesture = Some(Gesture::Move(screen, self.spec.placed));
        } else if let Some(entry) = self.entry(point).cloned() {
            self.selected = Some(entry.key.clone());
            if double {
                self.open(&entry);
            } else if !entry.is_recycle() {
                self.gesture = Some(Gesture::Item(screen));
            }
        } else {
            self.selected = None;
        }
        if self.gesture.is_some() {
            unsafe {
                SetCapture(self.hwnd);
            }
        }
        unsafe {
            SetFocus(Some(self.hwnd)).ok();
        }
        invalidate(self.hwnd);
    }

    fn move_pointer(&mut self) {
        if !self.hover {
            self.hover = true;
            self.opacity();
            unsafe {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: self.hwnd,
                    dwHoverTime: 0,
                });
            }
            self.schedule_hover();
        }
        let Some(gesture) = self.gesture else { return };
        let point = cursor();
        if let Gesture::Item(start) = gesture {
            let dragged = unsafe {
                (point.x - start.x).abs() >= GetSystemMetrics(SM_CXDRAG)
                    || (point.y - start.y).abs() >= GetSystemMetrics(SM_CYDRAG)
            };
            if dragged && let Some(entry) = self.selected().cloned() {
                self.cancel_gesture();
                dispatch(Action::Drag(self.hwnd, entry));
            }
            return;
        }
        let placed = self.move_bounds(gesture, point);
        self.position(placed, true);
        invalidate(self.hwnd);
    }

    fn move_bounds(&self, gesture: Gesture, point: POINT) -> Bounds {
        let (mut placed, moving) = match gesture {
            Gesture::Move(start, mut placed) => {
                placed.left += point.x - start.x;
                placed.top += point.y - start.y;
                (placed, true)
            }
            Gesture::Resize(start, mut placed) => {
                placed.width += point.x - start.x;
                placed.height += point.y - start.y;
                (placed, false)
            }
            Gesture::Item(_) => return self.spec.placed,
        };
        let work = work_area(Some(placed), moving.then_some(point));
        if moving && unsafe { GetKeyState(VK_MENU.0 as i32) } >= 0 {
            placed = snap(
                placed,
                work,
                &self.spec.neighbors,
                self.scale(12),
                self.scale(8),
            );
        }
        placed.fit(work, (self.scale(192), self.scale(125)))
    }

    fn cancel_gesture(&mut self) {
        self.gesture = None;
        unsafe {
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
        }
    }

    fn finish_gesture(&mut self) {
        if let Some(gesture @ (Gesture::Move(..) | Gesture::Resize(..))) = self.gesture {
            let placed = self.move_bounds(gesture, cursor());
            if placed != self.spec.placed {
                self.spec.placed = placed;
                dispatch(Action::Place(self.spec.frame.id.clone(), placed));
            }
        }
        self.cancel_gesture();
        self.schedule_hover();
    }

    fn schedule_hover(&self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), HOVER_TIMER);
            if self.spec.frame.collapsed
                && self.spec.frame.hover_expand
                && self.expanded != self.hover
                && self.gesture.is_none()
            {
                let delay = if self.hover {
                    self.spec.frame.expand_delay
                } else {
                    self.spec.frame.retract_delay
                };
                SetTimer(Some(self.hwnd), HOVER_TIMER, delay as u32, None);
            }
        }
    }

    fn scroll(&mut self, step: isize) {
        let grid = self.grid();
        let scroll = self
            .scroll
            .saturating_add_signed(step)
            .min(grid.limit(self.spec.entries.len()));
        if self.scroll != scroll {
            self.scroll = scroll;
            invalidate(self.hwnd);
        }
    }

    fn key(&mut self, key: VIRTUAL_KEY) {
        if key == VK_ESCAPE {
            self.cancel_gesture();
            self.position(self.spec.placed, false);
        } else if key == VK_RETURN {
            if let Some(entry) = self.selected() {
                self.open(entry);
            }
        } else if key == VK_BACK && self.spec.folder.is_some() {
            dispatch(Action::Back(self.spec.active.id.clone()));
        } else if key == VK_APPS {
            self.menu(cursor(), self.selected().cloned());
        } else if [
            VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_PRIOR, VK_NEXT, VK_HOME, VK_END,
        ]
        .contains(&key)
            && !self.collapsed()
            && !self.spec.entries.is_empty()
        {
            let grid = self.grid();
            let last = self.spec.entries.len() - 1;
            let next = self
                .selected
                .as_ref()
                .and_then(|key| self.spec.entries.iter().position(|entry| &entry.key == key))
                .map_or(0, |i| match key {
                    VK_LEFT => i.saturating_sub(1),
                    VK_RIGHT => (i + 1).min(last),
                    VK_UP => i.saturating_sub(grid.columns),
                    VK_DOWN => (i + grid.columns).min(last),
                    VK_PRIOR => i.saturating_sub(grid.rows * grid.columns),
                    VK_NEXT => (i + grid.rows * grid.columns).min(last),
                    VK_HOME => 0,
                    _ => last,
                });
            self.selected = Some(self.spec.entries[next].key.clone());
            let row = next / grid.columns;
            self.scroll = self.scroll.min(row).max(row.saturating_sub(grid.rows - 1));
        }
        invalidate(self.hwnd);
    }

    pub fn plan(&self, point: POINT, internal: bool) -> Option<DropPlan> {
        let tab = self
            .tab_rects()
            .find(|(_, area)| contains(*area, point))
            .map(|(tab, _)| tab);
        let group = tab.unwrap_or(&self.spec.active);
        let grid = self.grid();
        let hit = grid.hit(point);
        if let Some((index, entry)) =
            hit.and_then(|i| self.spec.entries.get(i).map(|entry| (i, entry)))
            && (entry.folder || entry.is_recycle())
        {
            let area = grid.cell(index);
            let (offset, length) = if group.view == View::Icons {
                (point.x - area.left, grid.width)
            } else {
                (point.y - area.top, grid.height)
            };
            if self.spec.loose
                || group.folder.is_some()
                || (offset >= length / 4 && offset < length - length / 4)
            {
                return Some(DropPlan::Shell(entry.path.clone()));
            }
        }
        if let Some(folder) = &group.folder {
            return Some(DropPlan::Shell(
                if group.id == self.spec.active.id {
                    self.spec.folder.as_ref().unwrap_or(folder)
                } else {
                    folder
                }
                .clone(),
            ));
        }
        if self.spec.loose {
            return None;
        }
        let insertion = if tab.is_none()
            && !self.collapsed()
            && contains(grid.zone, point)
            && (internal || hit.is_some_and(|index| index < self.spec.entries.len()))
        {
            let index = hit
                .unwrap_or(self.spec.entries.len())
                .min(self.spec.entries.len().saturating_sub(1));
            let cell = grid.cell(index);
            let icons = group.view == View::Icons;
            let after = hit.is_none_or(|hit| hit >= self.spec.entries.len())
                || self.spec.entries.get(index).is_some_and(Entry::is_recycle)
                || if icons {
                    point.x >= cell.left + grid.width / 2
                } else {
                    point.y >= cell.top + grid.height / 2
                };
            let thickness = self.scale(2);
            let mark = if icons {
                rect(
                    if after {
                        cell.right - thickness
                    } else {
                        cell.left
                    },
                    cell.top + self.scale(3),
                    thickness,
                    grid.height - self.scale(6),
                )
            } else {
                rect(
                    cell.left,
                    if after {
                        cell.bottom - thickness
                    } else {
                        cell.top
                    },
                    grid.width,
                    thickness,
                )
            };
            Some((
                (index + usize::from(after)).min(self.spec.entries.len()),
                mark,
            ))
        } else {
            None
        };
        Some(DropPlan::Assign {
            group: group.id.clone(),
            slot: insertion.map(|inf| inf.0),
            entries: if group.id == self.spec.active.id {
                self.spec.entries.clone()
            } else {
                Rc::new(Vec::new())
            },
            mark: insertion.map(|inf| inf.1),
        })
    }

    pub fn mark_drop(&mut self, point: Option<POINT>, mark: Option<RECT>) {
        if self.drop_mark != mark {
            self.drop_mark = mark;
            invalidate(self.hwnd);
        }
        if self.drop_point.is_some() != point.is_some() {
            unsafe {
                if point.is_some() {
                    SetTimer(Some(self.hwnd), SCROLL_TIMER, 115, None);
                } else {
                    let _ = KillTimer(Some(self.hwnd), SCROLL_TIMER);
                }
            }
        }
        self.drop_point = point;
    }

    fn scroll_drop(&mut self) {
        let Some(point) = self.drop_point else { return };
        let grid = self.grid();
        if !contains(grid.zone, point) {
            return;
        }
        let edge = self.scale(16).min(grid.height / 3);
        if point.y < grid.zone.top + edge {
            self.scroll(-1);
        } else if point.y >= grid.zone.bottom - edge {
            self.scroll(1);
        }
    }
}

fn snap(bounds: Bounds, work: Bounds, neighbors: &[Bounds], limit: i32, gap: i32) -> Bounds {
    let mut dx = vec![work.left - bounds.left, work.right() - bounds.right()];
    let mut dy = vec![work.top - bounds.top, work.bottom() - bounds.bottom()];
    for other in neighbors {
        if bounds.top <= other.bottom() + limit && bounds.bottom() >= other.top - limit {
            dx.extend([
                other.left - bounds.left,
                other.right() - bounds.right(),
                other.right() + gap - bounds.left,
                other.left - gap - bounds.right(),
            ]);
        }
        if bounds.left <= other.right() + limit && bounds.right() >= other.left - limit {
            dy.extend([
                other.top - bounds.top,
                other.bottom() - bounds.bottom(),
                other.bottom() + gap - bounds.top,
                other.top - gap - bounds.bottom(),
            ]);
        }
    }
    let nearest = |offsets: Vec<i32>| {
        offsets
            .into_iter()
            .filter(|offset| offset.abs() <= limit)
            .min_by_key(|offset| offset.abs())
            .unwrap_or(0)
    };
    Bounds {
        left: bounds.left + nearest(dx),
        top: bounds.top + nearest(dy),
        ..bounds
    }
}

pub struct Surface(HDC);

impl Surface {
    fn fill(&self, area: RECT, color: COLORREF) {
        unsafe {
            let brush = CreateSolidBrush(color);
            FillRect(self.0, &area, brush);
            let _ = DeleteObject(brush.into());
        }
    }

    fn text(&self, title: &str, mut area: RECT, color: COLORREF, style: DRAW_TEXT_FORMAT) {
        let mut text = wide(title);
        text.pop();
        unsafe {
            SetTextColor(self.0, color);
            DrawTextW(
                self.0,
                &mut text,
                &mut area,
                style | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
        }
    }

    fn icon(&self, entry: &Entry, x: i32, y: i32, size: i32, tint: Option<COLORREF>) {
        super::icons::draw(self.0, entry, x, y, size, tint);
    }
}

impl Panel {
    fn draw(&self, dc: HDC) {
        let area = client(self.hwnd);
        let canvas = Surface(dc);
        let accent = COLORS[self.spec.active.accent as usize];
        let single = DT_VCENTER | DT_SINGLELINE;
        let inset = self.scale(10);
        let font = unsafe {
            CreateFontW(
                -self.scale(14),
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Segoe UI"),
            )
        };
        let saved = unsafe { SaveDC(dc) };
        unsafe {
            SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
        }
        canvas.fill(area, if self.spec.loose { KEY } else { BG });
        if !self.spec.loose {
            let header = self.scale(36);
            canvas.fill(rect(0, 0, area.right, header), HEADER);
            canvas.fill(rect(0, 0, self.scale(3), header), accent);
            canvas.text(
                &self.spec.active.title,
                rect(inset, 0, area.right - self.scale(81), header),
                INK,
                single,
            );
            canvas.text("⋯", self.menu_rect(), INK, single | DT_CENTER);
            canvas.text(
                if self.collapsed() { "⌄" } else { "⌃" },
                self.collapse_rect(),
                MUTED,
                single | DT_CENTER,
            );
            for (tab, region) in self.tab_rects() {
                let selected = tab.id == self.spec.active.id;
                canvas.fill(region, if selected { SELECTED } else { HEADER });
                let mut label = region;
                label.left += self.scale(6);
                label.right -= self.scale(6);
                canvas.text(
                    &tab.title,
                    label,
                    if selected { INK } else { MUTED },
                    single | DT_CENTER,
                );
                if selected {
                    canvas.fill(
                        rect(
                            region.left,
                            region.bottom - 2,
                            region.right - region.left,
                            2,
                        ),
                        accent,
                    );
                }
            }
            if !self.collapsed()
                && let (Some(folder), Some(portal)) = (&self.spec.folder, self.portal_bar())
            {
                canvas.fill(portal, HEADER);
                canvas.text(
                    "‹",
                    rect(
                        portal.left,
                        portal.top,
                        self.scale(33),
                        portal.bottom - portal.top,
                    ),
                    INK,
                    single | DT_CENTER,
                );
                let mut label = portal;
                label.left += self.scale(36);
                label.right -= self.scale(4);
                canvas.text(
                    &folder.to_string_lossy(),
                    label,
                    MUTED,
                    single | DT_PATH_ELLIPSIS,
                );
            }
        }
        if !self.collapsed() {
            self.draw_items(&canvas, accent, single);
        }
        unsafe {
            let _ = RestoreDC(dc, saved);
            let _ = DeleteObject(font.into());
        }
    }

    fn draw_items(&self, canvas: &Surface, accent: COLORREF, single: DRAW_TEXT_FORMAT) {
        let grid = self.grid();
        let icon = self.scale(self.spec.active.icon_size);
        let pad = self.scale(5);
        let details = self.spec.active.view == View::Details && grid.width >= self.scale(270);
        let date_at = grid.zone.left + grid.width * 68 / 100;
        let size_at = grid.zone.left + grid.width * 48 / 100;
        if details {
            let line = grid.zone.top - self.scale(21);
            for (title, left, right) in [
                (tr!("名称", "名稱", "Name"), grid.zone.left + pad, size_at),
                (tr!("大小", "大小", "Size"), size_at, date_at),
                (
                    tr!("修改时间", "修改時間", "Modified"),
                    date_at,
                    grid.zone.right,
                ),
            ] {
                canvas.text(
                    title,
                    rect(left, line, right - left, self.scale(21)),
                    MUTED,
                    single,
                );
            }
        }
        let saved = unsafe { SaveDC(canvas.0) };
        unsafe {
            IntersectClipRect(
                canvas.0,
                grid.zone.left,
                grid.zone.top,
                grid.zone.right,
                grid.zone.bottom,
            );
        }
        let items = &self.spec.entries;
        if items.is_empty() {
            canvas.text(
                if self.spec.status.is_empty() {
                    tr!("暂无项目", "沒有項目", "No items")
                } else {
                    self.spec.status
                },
                rect(
                    grid.zone.left + pad,
                    grid.zone.top + pad,
                    grid.width - pad,
                    self.scale(28),
                ),
                MUTED,
                single,
            );
        }
        let first = grid.first * grid.columns;
        let visible = (grid.rows + 1) * grid.columns;
        for (index, entry) in items.iter().enumerate().skip(first).take(visible) {
            let cell = grid.cell(index);
            let selected = self.selected.as_ref() == Some(&entry.key);
            if selected {
                canvas.fill(cell, SELECTED);
            } else if self.spec.active.view != View::Icons && index % 2 != 0 {
                canvas.fill(cell, HEADER);
            }
            let tint = self.spec.active.tint_icons.then_some(accent);
            if self.spec.active.view == View::Icons || self.spec.loose {
                let x = cell.left + (grid.width - icon) / 2;
                canvas.icon(entry, x, cell.top + pad, icon, tint);
                let y = cell.top + icon + 2 * pad;
                let label = rect(cell.left + pad, y, grid.width - 2 * pad, cell.bottom - y);
                if self.spec.loose && !selected {
                    canvas.fill(
                        rect(label.left, y, label.right - label.left, self.scale(20)),
                        HEADER,
                    );
                }
                canvas.text(
                    &entry.label,
                    label,
                    INK,
                    DT_CENTER | DT_WORDBREAK | DT_EDITCONTROL,
                );
            } else {
                canvas.icon(
                    entry,
                    cell.left + pad,
                    cell.top + (grid.height - icon) / 2,
                    icon,
                    tint,
                );
                let left = cell.left + icon + pad * 2;
                let right = if details {
                    size_at - pad
                } else {
                    cell.right - pad
                };
                canvas.text(
                    &entry.label,
                    rect(left, cell.top, right - left, grid.height),
                    INK,
                    single,
                );
                if details {
                    if let Some(size) = entry.bytes {
                        canvas.text(
                            &file_size(size),
                            rect(size_at, cell.top, date_at - size_at - pad, grid.height),
                            MUTED,
                            single,
                        );
                    }
                    if let Some(date) = entry.modified.and_then(file_date) {
                        canvas.text(
                            &date,
                            rect(date_at, cell.top, cell.right - date_at, grid.height),
                            MUTED,
                            single,
                        );
                    }
                }
            }
        }
        if let Some(mark) = self.drop_mark {
            canvas.fill(mark, accent);
        }
        if grid.limit(items.len()) > 0 {
            let rail = grid.zone.bottom - grid.zone.top;
            let thumb = (rail * grid.rows as i32 / items.len().div_ceil(grid.columns) as i32)
                .max(self.scale(12))
                .min(rail);
            let y = grid.zone.top
                + (rail - thumb) * self.scroll as i32 / grid.limit(items.len()) as i32;
            canvas.fill(rect(grid.zone.right - 3, y, 3, thumb), accent);
        }
        unsafe {
            let _ = RestoreDC(canvas.0, saved);
        }
        if !self.spec.loose && !self.spec.frame.locked {
            let area = client(self.hwnd);
            for length in [5, 9, 13] {
                canvas.fill(
                    rect(
                        area.right - self.scale(length + 3),
                        area.bottom - self.scale(16 - length),
                        self.scale(length),
                        1,
                    ),
                    MUTED,
                );
            }
        }
    }
}

fn file_size(size: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size} B")
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

fn file_date(time: SystemTime) -> Option<String> {
    use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    use windows::Win32::System::Time::FileTimeToSystemTime;

    let epoch =
        SystemTime::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(11_644_473_600))?;
    let ticks = u64::try_from(time.duration_since(epoch).ok()?.as_nanos() / 100).ok()?;
    let source = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut local = FILETIME::default();
    let mut date = SYSTEMTIME::default();
    unsafe {
        FileTimeToLocalFileTime(&source, &mut local).ok()?;
        FileTimeToSystemTime(&local, &mut date).ok()?;
    }
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.wYear, date.wMonth, date.wDay
    ))
}

fn paint(window: HWND, panel: &Panel) {
    let mut paint = PAINTSTRUCT::default();
    unsafe {
        let dc = BeginPaint(window, &mut paint);
        let area = client(window);
        if area.right > 0 && area.bottom > 0 {
            let buffer = CreateCompatibleDC(Some(dc));
            let bitmap = CreateCompatibleBitmap(dc, area.right, area.bottom);
            if !buffer.is_invalid() && !bitmap.is_invalid() {
                let previous = SelectObject(buffer, bitmap.into());
                panel.draw(buffer);
                let _ = BitBlt(
                    dc,
                    0,
                    0,
                    area.right,
                    area.bottom,
                    Some(buffer),
                    0,
                    0,
                    SRCCOPY,
                );
                SelectObject(buffer, previous);
            } else {
                panel.draw(dc);
            }
            if !bitmap.is_invalid() {
                let _ = DeleteObject(bitmap.into());
            }
            if !buffer.is_invalid() {
                let _ = DeleteDC(buffer);
            }
        }
        let _ = EndPaint(window, &paint);
    }
}

pub unsafe extern "system" fn proc(hwnd: HWND, kind: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if kind == WM_NCDESTROY {
        unsafe {
            let _ = windows::Win32::System::Ole::RevokeDragDrop(hwnd);
        }
        PANELS.with(|panels| panels.borrow_mut().remove(&(hwnd.0 as isize)));
    }
    if let Some(state) = lookup(hwnd)
        && let Ok(mut panel) = state.try_borrow_mut()
    {
        match kind {
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => paint(hwnd, &panel),
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                panel.button_down(mouse_point(lp), kind == WM_LBUTTONDBLCLK)
            }
            WM_MOUSEMOVE => panel.move_pointer(),
            WM_LBUTTONUP => panel.finish_gesture(),
            WM_MOUSELEAVE => {
                panel.hover = false;
                panel.opacity();
                panel.schedule_hover();
            }
            WM_CAPTURECHANGED => {
                if panel.gesture.take().is_some() {
                    panel.position(panel.spec.placed, false);
                }
                panel.schedule_hover();
            }
            WM_MOUSEWHEEL => {
                let point = client_point(hwnd, mouse_point(lp));
                let step = (wp.0 >> 16) as i16 as isize;
                if panel.tab_rects().any(|(_, area)| contains(area, point)) {
                    panel.tab_start = panel
                        .tab_start
                        .saturating_add_signed(-step.signum())
                        .min(panel.spec.tabs.len() - 1);
                    invalidate(hwnd);
                } else {
                    panel.scroll(-step.signum() * (step.abs() / 120).max(1) * 2);
                }
            }
            WM_RBUTTONUP => panel.menu(cursor(), panel.entry(mouse_point(lp)).cloned()),
            WM_KEYDOWN => panel.key(VIRTUAL_KEY(wp.0 as u16)),
            WM_SIZE | WM_DPICHANGED | WM_DPICHANGED_AFTERPARENT => invalidate(hwnd),
            WM_TIMER if wp.0 == HOVER_TIMER => {
                unsafe {
                    let _ = KillTimer(Some(hwnd), HOVER_TIMER);
                }
                if panel.spec.frame.collapsed && panel.gesture.is_none() {
                    panel.expanded = panel.hover;
                    panel.position(panel.spec.placed, panel.expanded);
                    invalidate(hwnd);
                }
            }
            WM_TIMER if wp.0 == SCROLL_TIMER => panel.scroll_drop(),
            _ => return unsafe { DefWindowProcW(hwnd, kind, wp, lp) },
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, kind, wp, lp) }
}

#[cfg(test)]
pub(super) fn register_for_test() {
    static CLASS_REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    CLASS_REGISTERED.get_or_init(|| register(CLASS, Some(proc), CS_DBLCLKS).unwrap());
}

#[cfg(test)]
fn test_spec(placed: Bounds) -> Spec {
    let mut frame = Group::new("桌面格显示测试", 0, placed, "desktop");
    frame.opacity = 100;
    Spec {
        frame: frame.clone(),
        active: frame,
        tabs: Vec::new(),
        folder: None,
        entries: Rc::default(),
        status: "",
        placed,
        neighbors: Rc::default(),
        loose: false,
    }
}

#[cfg(test)]
fn lower_panel(hwnd: HWND) {
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_BOTTOM),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
    }
}

#[cfg(test)]
#[test]
fn showing_a_child_panel_raises_it_above_desktop_siblings() {
    let _ole = super::Ole::new().unwrap();
    register_for_test();
    unsafe {
        let host = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("Static"),
            None,
            WS_POPUP,
            0,
            0,
            500,
            400,
            None,
            None,
            Some(instance()),
            None,
        )
        .unwrap();
        let wallpaper = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("Static"),
            None,
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            500,
            400,
            Some(host),
            None,
            Some(instance()),
            None,
        )
        .unwrap();
        let spec = test_spec(Bounds {
            left: 20,
            top: 20,
            width: 280,
            height: 175,
        });
        let panel = create(host, false, spec.clone()).unwrap();
        update(panel, spec);
        lower_panel(panel);

        show(panel, true);
        let startup_order = GetWindow(host, GW_CHILD).unwrap();
        lower_panel(panel);
        let covered_order = GetWindow(host, GW_CHILD).unwrap();
        show(panel, true);
        let restored_order = GetWindow(host, GW_CHILD).unwrap();
        show(panel, false);
        let stayed_hidden = !visible_style(panel);
        DestroyWindow(host).unwrap();

        assert_eq!(startup_order, panel);
        assert_eq!(covered_order, wallpaper);
        assert_eq!(restored_order, panel);
        assert!(stayed_hidden);
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires dist/DeskGrid.exe and an exposed interactive Windows desktop"]
fn release_panel_paints_above_explorer_layers() {
    struct TestProcess(std::process::Child);
    impl Drop for TestProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let _ole = super::Ole::new().unwrap();
    let e = explorer();
    assert!(!e.host.is_invalid(), "Explorer desktop is unavailable");
    let icons = e
        .icons()
        .expect("could not read native desktop icon bounds");
    let work = work_area(None, None);
    let sample_offset = POINT { x: 190, y: 30 };
    let point = (0..8)
        .flat_map(|row| {
            (0..8).map(move |col| POINT {
                x: work.left + 35 + col * (work.width - 300) / 8,
                y: work.top + 35 + row * (work.height - 220) / 8,
            })
        })
        .find(|point| {
            e.blank(
                POINT {
                    x: point.x + sample_offset.x,
                    y: point.y + sample_offset.y,
                },
                Some(&icons),
            )
        })
        .expect("no exposed desktop area to test");
    let at = POINT {
        x: point.x + sample_offset.x,
        y: point.y + sample_offset.y,
    };
    let placed = Bounds {
        left: point.x,
        top: point.y,
        width: 280,
        height: 175,
    };
    let data = tempfile::tempdir().unwrap();
    let desktop = data.path().join("Desktop");
    std::fs::create_dir(&desktop).unwrap();
    std::fs::write(desktop.join("测试.txt"), "DeskGrid").unwrap();
    let mut layout = crate::layout::Layout::initial(work, 96);
    layout.groups.items = vec![test_spec(placed).frame];
    layout.default_group_id = layout.groups.items[0].id.clone();
    layout.rules.items.clear();
    crate::layout::write(&data.path().join("layout.xml"), &layout, false).unwrap();
    let executable = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dist/DeskGrid.exe");
    let process = TestProcess(
        std::process::Command::new(executable)
            .arg("--autostart")
            .env("DESKGRID_DATA_DIR", data.path())
            .env("DESKGRID_DESKTOP_DIR", desktop)
            .spawn()
            .expect("build the release executable before running this test"),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    let (hit_pid, pixel) = loop {
        let mut hit_pid = 0;
        let pixel = unsafe {
            GetWindowThreadProcessId(WindowFromPoint(at), Some(&mut hit_pid));
            let screen = GetDC(None);
            let pixel = GetPixel(screen, at.x, at.y);
            let _ = ReleaseDC(None, screen);
            pixel
        };
        if (hit_pid == process.0.id() && pixel == HEADER) || std::time::Instant::now() >= deadline {
            break (hit_pid, pixel);
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    };
    assert_eq!(
        hit_pid,
        process.0.id(),
        "the panel must receive input at startup"
    );
    assert_eq!(pixel, HEADER, "the panel header must be visible on screen");
}
