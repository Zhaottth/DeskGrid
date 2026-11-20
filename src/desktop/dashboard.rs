use std::cell::Cell;
use std::ffi::OsStr;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::layout::Layout;

use super::dialog::{message, set_text, template};
use super::platform::*;
use super::theme::{self, Button, Canvas};
use super::{Action, dispatch, panel};

const LIST: i32 = 100;
const NEW: i32 = 101;
const FOLDER: i32 = 102;
const EDIT: i32 = 103;
const REMOVE: i32 = 104;
const ALIGN: i32 = 105;
const PAGES: i32 = 106;
const RULES: i32 = 107;
const OPTIONS: i32 = 108;
const SEARCH: i32 = 109;
const IMPORT: i32 = 110;
const EXPORT: i32 = 111;
const EXIT: i32 = 112;
const NEW_PAGE: i32 = 113;

const W: i32 = 770;
const H: i32 = 526;

thread_local! {
    static WINDOW: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
}

pub fn window() -> HWND {
    WINDOW.with(Cell::get)
}

struct Row {
    id: String,
    title: String,
    portal: bool,
    accent: i32,
}

#[derive(Default)]
struct Dashboard {
    rows: Vec<Row>,
    page: String,
}

fn buttons() -> [(i32, &'static str, [i32; 4]); 13] {
    [
        (
            PAGES,
            tr!("管理页面", "管理頁面", "Pages"),
            [14, 131, 152, 37],
        ),
        (
            RULES,
            tr!("分类规则", "分類規則", "Rules"),
            [14, 172, 152, 37],
        ),
        (OPTIONS, tr!("设置", "設定", "Settings"), [14, 213, 152, 37]),
        (SEARCH, tr!("搜索", "搜尋", "Search"), [14, 254, 152, 37]),
        (
            IMPORT,
            tr!("导入布局", "匯入版面", "Import layout"),
            [14, 327, 152, 37],
        ),
        (
            EXPORT,
            tr!("导出布局", "匯出版面", "Export layout"),
            [14, 368, 152, 37],
        ),
        (
            EXIT,
            tr!("退出桌面格", "結束桌面格", "Quit DeskGrid"),
            [14, 471, 152, 37],
        ),
        (
            NEW_PAGE,
            tr!("＋ 新建页面", "＋ 新增頁面", "+ New page"),
            [613, 66, 128, 33],
        ),
        (
            NEW,
            tr!("＋ 新建分组", "＋ 新增群組", "+ New group"),
            [208, 112, 158, 38],
        ),
        (
            FOLDER,
            tr!("添加文件夹入口", "新增資料夾入口", "Add folder portal"),
            [376, 112, 175, 38],
        ),
        (
            ALIGN,
            tr!("批量对齐", "批次對齊", "Align panels"),
            [613, 112, 128, 38],
        ),
        (
            EDIT,
            tr!("编辑面板", "編輯面板", "Edit panel"),
            [208, 366, 140, 36],
        ),
        (
            REMOVE,
            tr!("移除面板", "移除面板", "Remove panel"),
            [357, 366, 142, 36],
        ),
    ]
}

fn window_title() -> &'static str {
    tr!("桌面格 · 管理", "桌面格 · 管理", "DeskGrid · Manager")
}

fn control(hwnd: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(Some(hwnd), id).unwrap_or_default() }
}

fn state(hwnd: HWND) -> *mut Dashboard {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Dashboard }
}

pub fn open() -> windows::core::Result<HWND> {
    let template = template(425, 300, false);
    let hwnd = unsafe {
        CreateDialogIndirectParamW(
            Some(instance()),
            template.as_ptr().cast(),
            None,
            Some(proc),
            LPARAM(0),
        )?
    };
    let work = work_area(None, Some(cursor()));
    let scale = monitor_dpi(cursor());
    let mut size = rect(0, 0, px(W, scale), px(H, scale));
    unsafe {
        use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
        AdjustWindowRectExForDpi(
            &mut size,
            WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32),
            false,
            WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32),
            scale as u32,
        )?;
        let width = size.right - size.left;
        let height = size.bottom - size.top;
        let x = work.left + (work.width - width).max(0) / 2;
        let y = work.top + (work.height - height).max(0) / 2;
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
    }
    position(hwnd);
    Ok(hwnd)
}

fn child(hwnd: HWND, class: PCWSTR, id: i32, label: &str, style: u32) {
    unsafe {
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(wide(label).as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(style),
            0,
            0,
            0,
            0,
            Some(hwnd),
            Some(HMENU(id as usize as *mut _)),
            Some(instance()),
            None,
        );
    }
}

fn position(hwnd: HWND) {
    let dpi = dpi(hwnd);
    let move_child = |id, bounds: [i32; 4]| {
        let [x, y, w, h] = bounds.map(|value| px(value, dpi));
        unsafe {
            let _ = MoveWindow(control(hwnd, id), x, y, w, h, true);
        }
    };
    for (id, _, bounds) in buttons() {
        move_child(id, bounds);
    }
    move_child(LIST, [209, 166, 531, 184]);
    message(
        control(hwnd, LIST),
        LB_SETITEMHEIGHT,
        0,
        px(46, dpi) as isize,
    );
}

pub fn refresh(hwnd: HWND, layout: &Layout) {
    let pointer = state(hwnd);
    if pointer.is_null() {
        return;
    }
    let list = control(hwnd, LIST);
    let index = message(list, LB_GETCURSEL, 0, 0).max(0) as usize;
    let (rows, selected) = {
        let data = unsafe { &mut *pointer };
        let selected = data.rows.get(index).map(|row| row.id.clone());
        data.page = layout
            .pages
            .items
            .iter()
            .find(|page| page.id == layout.active_page_id)
            .map_or_else(String::new, |page| page.title.clone());
        data.rows = layout
            .groups
            .items
            .iter()
            .map(|group| Row {
                id: group.id.clone(),
                title: group.title.clone(),
                portal: group.folder.is_some(),
                accent: group.accent,
            })
            .collect();
        let selected = data
            .rows
            .iter()
            .position(|row| Some(&row.id) == selected.as_ref())
            .unwrap_or_else(|| index.min(data.rows.len().saturating_sub(1)));
        (
            data.rows
                .iter()
                .map(|row| row.title.clone())
                .collect::<Vec<_>>(),
            selected,
        )
    };
    set_text(hwnd, OsStr::new(window_title()));
    for (id, label, _) in buttons() {
        set_text(control(hwnd, id), OsStr::new(label));
    }
    message(list, WM_SETREDRAW, 0, 0);
    message(list, LB_RESETCONTENT, 0, 0);
    for title in rows {
        message(list, LB_ADDSTRING, 0, wide(title).as_ptr() as isize);
    }
    message(list, LB_SETCURSEL, selected, 0);
    message(list, WM_SETREDRAW, 1, 0);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
        let _ = InvalidateRect(Some(list), None, true);
    }
}

fn paint(hwnd: HWND, data: &Dashboard) {
    unsafe {
        let mut item = PAINTSTRUCT::default();
        let canvas = Canvas {
            dc: BeginPaint(hwnd, &mut item),
            dpi: dpi(hwnd),
        };
        canvas.fill(client(hwnd), theme::BG);
        canvas.fill(canvas.at(0, 0, 180, H), theme::RAIL);
        canvas.fill(canvas.at(179, 0, 1, H), theme::EDGE);
        canvas.card(canvas.at(16, 25, 36, 36), theme::SELECTED, theme::EDGE, 9);
        canvas.icon(
            resource_icon(),
            canvas.unit(22),
            canvas.unit(31),
            canvas.unit(24),
        );
        canvas.text(
            "DeskGrid",
            canvas.at(62, 24, 100, 36),
            theme::TEXT,
            21,
            700,
            DT_LEFT,
        );

        canvas.card(
            canvas.at(14, 90, 152, 37),
            theme::SELECTED,
            theme::SELECTED,
            7,
        );
        canvas.text(
            tr!("面板", "面板", "Panels"),
            canvas.at(30, 90, 123, 37),
            theme::ACCENT,
            14,
            700,
            DT_LEFT,
        );
        canvas.fill(canvas.at(24, 309, 132, 1), theme::EDGE);

        canvas.text(
            tr!("面板管理", "面板管理", "Manage panels"),
            canvas.at(208, 26, 530, 42),
            theme::TEXT,
            30,
            700,
            DT_LEFT,
        );
        let page = &data.page;
        let subtitle = tr_format!(
            "当前页面 · {page}",
            "目前頁面 · {page}",
            "Current page · {page}"
        );
        canvas.text(
            &subtitle,
            canvas.at(209, 70, 393, 25),
            theme::MUTED,
            15,
            400,
            DT_LEFT,
        );
        canvas.card(canvas.at(208, 165, 533, 186), theme::WELL, theme::EDGE, 6);

        canvas.card(canvas.at(208, 431, 533, 70), theme::CARD, theme::EDGE, 9);
        canvas.text(
            tr!(
                "在当前页面绘制分组",
                "在目前頁面繪製群組",
                "Draw a group on this page"
            ),
            canvas.at(225, 440, 497, 25),
            theme::ACCENT,
            15,
            700,
            DT_LEFT,
        );
        canvas.text(
            tr!(
                "在桌面空白处按住右键拖框，再松开右键。",
                "在桌面空白處按住右鍵拖曳，畫出範圍後鬆開右鍵。",
                "Right-drag a rectangle on an empty area of the desktop."
            ),
            canvas.at(225, 466, 497, 22),
            theme::MUTED,
            14,
            400,
            DT_LEFT,
        );
        let _ = EndPaint(hwnd, &item);
    }
}

fn draw_row(hwnd: HWND, item: &DRAWITEMSTRUCT, data: &Dashboard) {
    let canvas = Canvas {
        dc: item.hDC,
        dpi: dpi(hwnd),
    };
    let area = item.rcItem;
    canvas.fill(area, theme::WELL);
    let Some(row) = data.rows.get(item.itemID as usize) else {
        return;
    };
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    let mut background = area;
    background.left += canvas.unit(3);
    background.right -= canvas.unit(3);
    background.top += canvas.unit(1);
    background.bottom -= canvas.unit(1);
    if selected {
        let border = if item.itemState.0 & ODS_FOCUS.0 != 0 {
            theme::ACCENT
        } else {
            theme::SELECTED
        };
        canvas.card(background, theme::SELECTED, border, 6);
    } else {
        canvas.fill(
            rect(
                area.left + canvas.unit(13),
                area.bottom - 1,
                area.right - area.left - canvas.unit(26),
                1,
            ),
            theme::EDGE,
        );
    }
    let accent = panel::COLORS[row.accent.clamp(0, 7) as usize];
    let size = canvas.unit(8);
    let y = (area.top + area.bottom - size) / 2;
    canvas.card(
        rect(area.left + canvas.unit(15), y, size, size),
        accent,
        accent,
        4,
    );
    let mut name = area;
    name.left += canvas.unit(35);
    name.right -= canvas.unit(125);
    canvas.text(&row.title, name, theme::TEXT, 15, 600, DT_LEFT);
    let mut kind = area;
    kind.left = name.right;
    kind.right -= canvas.unit(16);
    let label = if row.portal {
        tr!("文件夹入口", "資料夾入口", "Folder portal")
    } else {
        tr!("分组", "群組", "Group")
    };
    canvas.text(label, kind, theme::MUTED, 13, 400, DT_RIGHT);
}

fn command(hwnd: HWND, wp: WPARAM, data: &Dashboard) -> bool {
    let id = (wp.0 & 0xffff) as i32;
    let notification = (wp.0 >> 16) as u32;
    let selected = || {
        let index = usize::try_from(message(control(hwnd, LIST), LB_GETCURSEL, 0, 0)).ok()?;
        data.rows.get(index).map(|row| row.id.clone())
    };
    let command = match id {
        LIST if notification == LBN_DBLCLK => selected().map(Action::EditGroup),
        EDIT => selected().map(Action::EditGroup),
        REMOVE => selected().map(Action::RemoveGroup),
        NEW => Some(Action::NewGroup(false)),
        NEW_PAGE => Some(Action::NewPage),
        FOLDER => Some(Action::NewGroup(true)),
        ALIGN => Some(Action::Align),
        PAGES => Some(Action::Pages),
        RULES => Some(Action::Rules),
        OPTIONS => Some(Action::Options),
        SEARCH => Some(Action::Search(None)),
        IMPORT => Some(Action::Import),
        EXPORT => Some(Action::Export),
        EXIT => Some(Action::Exit),
        _ => return false,
    };
    if let Some(command) = command {
        dispatch(command);
    }
    true
}

unsafe extern "system" fn proc(hwnd: HWND, kind: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if kind == WM_INITDIALOG {
        unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                Box::into_raw(Box::<Dashboard>::default()) as isize,
            );
        }
        theme::frame(hwnd);
        WINDOW.with(|window| window.set(hwnd));
        message(
            hwnd,
            WM_SETICON,
            ICON_SMALL as usize,
            resource_icon().0 as isize,
        );
        message(
            hwnd,
            WM_SETICON,
            ICON_BIG as usize,
            resource_icon().0 as isize,
        );
        for (id, label, _) in buttons() {
            child(hwnd, w!("Button"), id, label, BS_OWNERDRAW as u32);
        }
        child(
            hwnd,
            w!("ListBox"),
            LIST,
            "",
            WS_VSCROLL.0
                | LBS_NOTIFY as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_NOINTEGRALHEIGHT as u32
                | LBS_HASSTRINGS as u32,
        );
        unsafe {
            let _ = SetFocus(Some(control(hwnd, LIST)));
        }
        return 0;
    }
    if kind == WM_CLOSE || (kind == WM_COMMAND && wp.0 == IDCANCEL.0 as usize) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return 1;
    }
    let pointer = state(hwnd);
    if pointer.is_null() {
        return 0;
    }
    match kind {
        WM_NCDESTROY => unsafe {
            WINDOW.with(|window| window.set(HWND::default()));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(pointer));
            0
        },
        WM_COMMAND => isize::from(command(hwnd, wp, unsafe { &*pointer })),
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd, unsafe { &*pointer });
            1
        }
        WM_CTLCOLORLISTBOX => unsafe {
            let dc = HDC(wp.0 as *mut _);
            SetBkColor(dc, theme::WELL);
            SetDCBrushColor(dc, theme::WELL);
            GetStockObject(DC_BRUSH).0 as isize
        },
        WM_DRAWITEM => {
            let item = unsafe { &*(lp.0 as *const DRAWITEMSTRUCT) };
            let id = item.CtlID as i32;
            if id == LIST {
                draw_row(hwnd, item, unsafe { &*pointer });
            } else {
                let (appearance, background) = match id {
                    NEW => (Button::Primary, theme::BG),
                    PAGES..=EXIT => (Button::Navigation, theme::RAIL),
                    _ => (Button::Outline, theme::BG),
                };
                theme::button(item, dpi(hwnd), appearance, background);
            }
            1
        }
        WM_DPICHANGED => {
            let area = unsafe { &*(lp.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    area.left,
                    area.top,
                    area.right - area.left,
                    area.bottom - area.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            position(hwnd);
            1
        }
        _ => 0,
    }
}
