use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::platform::{client, instance, wide};

pub enum Input {
    Text(OsString),
    Folder(OsString),
    Choice(Vec<String>, usize),
    Check(bool),
}

pub enum Value {
    Text(OsString),
    Choice(usize),
    Check(bool),
}

impl Value {
    pub fn text(&self) -> &OsStr {
        match self {
            Self::Text(text) => text,
            _ => OsStr::new(""),
        }
    }

    pub fn choice(&self) -> usize {
        match self {
            Self::Choice(value) => *value,
            _ => 0,
        }
    }

    pub fn checked(&self) -> bool {
        matches!(self, Self::Check(true))
    }
}

struct Form<'a> {
    title: &'a str,
    fields: Vec<(&'a str, Input)>,
    controls: Vec<HWND>,
    buttons: &'a [&'a str],
    result: Option<(usize, Vec<Value>)>,
}

pub(super) fn template(width: u16, height: u16, visible: bool) -> Vec<u32> {
    let mut style =
        (WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN).0 | (DS_CENTER | DS_SETFONT) as u32;
    if visible {
        style |= WS_VISIBLE.0 | (DS_MODALFRAME | DS_SETFOREGROUND) as u32;
    }
    // Pack the standard dialog's UTF-16 fields into DWORDs for pointer alignment.
    let extra = [0, 0, 0, 9]
        .into_iter()
        .chain("Segoe UI".encode_utf16())
        .chain([0]);
    let words: Vec<_> = [
        style as u16,
        (style >> 16) as u16,
        0,
        0,
        0,
        0,
        0,
        width,
        height,
    ]
    .into_iter()
    .chain(extra)
    .collect();
    words
        .chunks(2)
        .map(|pair| u32::from(pair[0]) | (u32::from(pair.get(1).copied().unwrap_or(0)) << 16))
        .collect()
}

fn modal(owner: HWND, height: u16, function: DLGPROC, state: LPARAM) -> isize {
    let template = template(365, height, true);
    let result = unsafe {
        DialogBoxIndirectParamW(
            Some(instance()),
            template.as_ptr().cast(),
            visible_owner(owner),
            function,
            state,
        )
    };
    if result == -1 {
        super::report_error(&tr_format!(
            "无法打开桌面格对话框。{}",
            "無法開啟桌面格對話框。{}",
            "Could not open the DeskGrid dialog. {}",
            windows::core::Error::from_thread()
        ));
    }
    result
}

fn visible_owner(owner: HWND) -> Option<HWND> {
    unsafe { IsWindowVisible(owner) }.as_bool().then_some(owner)
}

fn units(hwnd: HWND, mut rect: RECT) -> RECT {
    unsafe {
        let _ = MapDialogRect(hwnd, &mut rect);
    }
    rect
}

fn control(hwnd: HWND, class: PCWSTR, title: &OsStr, style: u32, id: usize, rect: RECT) -> HWND {
    let rect = units(hwnd, rect);
    let is_edit = unsafe {
        class.as_wide() == w!("Edit").as_wide() || class.as_wide() == w!("ListBox").as_wide()
    };
    unsafe {
        let control = CreateWindowExW(
            if is_edit {
                WS_EX_CLIENTEDGE
            } else {
                WINDOW_EX_STYLE(0)
            },
            class,
            PCWSTR(wide(title).as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            Some(hwnd),
            Some(HMENU(id as *mut _)),
            Some(instance()),
            None,
        )
        .unwrap_or_default();
        let font = SendMessageW(hwnd, WM_GETFONT, Some(WPARAM(0)), Some(LPARAM(0)));
        if font.0 != 0 {
            SendMessageW(
                control,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        } else {
            let font = windows::Win32::Graphics::Gdi::GetStockObject(
                windows::Win32::Graphics::Gdi::DEFAULT_GUI_FONT,
            );
            SendMessageW(
                control,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
        control
    }
}

fn box_at(x: i32, y: i32, width: i32, height: i32) -> RECT {
    super::platform::rect(x, y, width, height)
}

fn button(hwnd: HWND, title: &str, id: usize, rect: RECT, primary: bool) -> HWND {
    control(
        hwnd,
        w!("Button"),
        OsStr::new(title),
        WS_TABSTOP.0
            | if primary {
                BS_DEFPUSHBUTTON as u32
            } else {
                BS_PUSHBUTTON as u32
            },
        id,
        rect,
    )
}

pub(super) fn message(hwnd: HWND, kind: u32, wparam: usize, lparam: isize) -> isize {
    unsafe { SendMessageW(hwnd, kind, Some(WPARAM(wparam)), Some(LPARAM(lparam))).0 }
}

fn text(hwnd: HWND) -> OsString {
    unsafe {
        let mut content = vec![0; GetWindowTextLengthW(hwnd).max(0) as usize + 1];
        let length = GetWindowTextW(hwnd, &mut content).max(0) as usize;
        OsString::from_wide(&content[..length])
    }
}

pub(super) fn set_text(hwnd: HWND, title: &OsStr) {
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide(title).as_ptr()));
    }
}

fn form_init(hwnd: HWND, data: &mut Form<'_>) {
    set_text(hwnd, OsStr::new(data.title));
    for (i, (label, field)) in data.fields.iter().enumerate() {
        let y = 8 + i as i32 * 19;
        let id = 100 + i;
        let window = match field {
            Input::Check(value) => {
                let window = control(
                    hwnd,
                    w!("Button"),
                    OsStr::new(label),
                    WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
                    id,
                    box_at(8, y, 347, 15),
                );
                message(window, BM_SETCHECK, usize::from(*value), 0);
                window
            }
            _ => {
                control(
                    hwnd,
                    w!("Static"),
                    OsStr::new(label),
                    0,
                    0,
                    box_at(8, y + 2, 125, 15),
                );
                match field {
                    Input::Text(value) | Input::Folder(value) => {
                        let folder = matches!(field, Input::Folder(_));
                        let width = if folder { 157 } else { 217 };
                        if folder {
                            button(
                                hwnd,
                                tr!("浏览…", "瀏覽…", "Browse…"),
                                300 + i,
                                box_at(301, y, 54, 15),
                                false,
                            );
                        }
                        control(
                            hwnd,
                            w!("Edit"),
                            value,
                            WS_TABSTOP.0 | ES_AUTOHSCROLL as u32,
                            id,
                            box_at(138, y, width, 15),
                        )
                    }
                    Input::Choice(options, value) => {
                        let window = control(
                            hwnd,
                            w!("ComboBox"),
                            OsStr::new(""),
                            WS_TABSTOP.0 | WS_VSCROLL.0 | CBS_DROPDOWNLIST as u32,
                            id,
                            box_at(138, y, 217, 150),
                        );
                        for option in options {
                            message(window, CB_ADDSTRING, 0, wide(option).as_ptr() as isize);
                        }
                        message(window, CB_SETCURSEL, *value, 0);
                        window
                    }
                    Input::Check(_) => unreachable!(),
                }
            }
        };
        data.controls.push(window);
    }
    let y = 11 + data.fields.len() as i32 * 19;
    for (i, label) in data.buttons.iter().enumerate() {
        let id = if i == 0 { IDOK.0 as usize } else { 500 + i };
        let row = i as i32 / 4;
        let col = i as i32 % 4;
        button(
            hwnd,
            label,
            id,
            box_at(8 + 89 * col, y + 20 * row, 80, 16),
            i == 0,
        );
    }
    let close_y = y + 20 * data.buttons.len().div_ceil(4) as i32;
    button(
        hwnd,
        tr!("关闭", "關閉", "Close"),
        IDCANCEL.0 as usize,
        box_at(275, close_y, 80, 16),
        false,
    );
}

unsafe extern "system" fn form_proc(hwnd: HWND, kind: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if kind == WM_INITDIALOG {
        let form = unsafe { &mut *(lp.0 as *mut Form<'_>) };
        form_init(hwnd, form);
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lp.0);
        }
        if let Some(&control) = form.controls.first() {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(control));
            }
            return 0;
        }
        return 1;
    }
    if kind != WM_COMMAND {
        return 0;
    }
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Form<'_> };
    if state.is_null() {
        return 0;
    }
    let id = wp.0 & 0xffff;
    if id == IDCANCEL.0 as usize {
        unsafe {
            let _ = EndDialog(hwnd, 0);
        }
        return 1;
    }
    if (300..500).contains(&id)
        && let Some(&edit) = unsafe { &*state }.controls.get(id - 300)
    {
        if let Some(path) = choose_path(hwnd, PathMode::Folder) {
            set_text(edit, path.as_os_str());
        }
        return 1;
    }
    let data = unsafe { &mut *state };
    let chosen = if id == IDOK.0 as usize {
        0
    } else if id >= 501 {
        id - 500
    } else {
        return 0;
    };
    if chosen >= data.buttons.len() {
        return 0;
    }
    let values = data
        .fields
        .iter()
        .zip(&data.controls)
        .map(|((_, field), control)| match field {
            Input::Text(_) | Input::Folder(_) => Value::Text(text(*control)),
            Input::Choice(_, _) => Value::Choice(
                usize::try_from(message(*control, CB_GETCURSEL, 0, 0)).unwrap_or(usize::MAX),
            ),
            Input::Check(_) => Value::Check(message(*control, BM_GETCHECK, 0, 0) == 1),
        })
        .collect();
    data.result = Some((chosen, values));
    unsafe {
        let _ = EndDialog(hwnd, 1);
    }
    1
}

pub fn form(
    owner: HWND,
    title: &str,
    fields: Vec<(&str, Input)>,
    buttons: &[&str],
) -> Option<(usize, Vec<Value>)> {
    let mut data = Form {
        title,
        fields,
        controls: Vec::new(),
        buttons,
        result: None,
    };
    let height = 32 + data.fields.len() * 19 + 20 * buttons.len().div_ceil(4);
    modal(
        owner,
        height as u16,
        Some(form_proc),
        LPARAM(&mut data as *mut Form<'_> as isize),
    );
    data.result
}

pub enum PathMode {
    Folder,
    Import,
    Export,
}

pub fn choose_path(owner: HWND, mode: PathMode) -> Option<PathBuf> {
    let result = unsafe {
        let dialog: IFileDialog = CoCreateInstance(
            if matches!(mode, PathMode::Export) {
                &FileSaveDialog
            } else {
                &FileOpenDialog
            },
            None,
            CLSCTX_INPROC_SERVER,
        )
        .ok()?;
        let options = dialog.GetOptions().ok()? | FOS_FORCEFILESYSTEM | FOS_NOCHANGEDIR;
        if matches!(mode, PathMode::Folder) {
            dialog
                .SetOptions(options | FOS_PICKFOLDERS | FOS_PATHMUSTEXIST)
                .ok()?;
        } else {
            dialog.SetOptions(options).ok()?;
            dialog
                .SetFileTypes(&[Common::COMDLG_FILTERSPEC {
                    pszName: PCWSTR(
                        wide(tr!("DeskGrid 布局", "DeskGrid 版面", "DeskGrid layout")).as_ptr(),
                    ),
                    pszSpec: w!("*.xml"),
                }])
                .ok()?;
            dialog.SetDefaultExtension(w!("xml")).ok()?;
            if matches!(mode, PathMode::Export) {
                dialog.SetFileName(w!("DeskGrid-layout.xml")).ok()?;
            }
        }
        dialog.Show(visible_owner(owner)).ok()?;
        let item = dialog.GetResult().ok()?;
        let value = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let result = OsString::from_wide(value.as_wide());
        CoTaskMemFree(Some(value.0.cast()));
        result
    };
    Some(Path::new(&result).to_owned())
}

struct Search<'a> {
    rows: &'a [(String, String)],
    edit: HWND,
    list: HWND,
    filtered: Vec<usize>,
    result: Option<(usize, bool)>,
}

fn filter(data: &mut Search<'_>) {
    let query = text(data.edit).to_string_lossy().trim().to_lowercase();
    data.filtered.clear();
    message(data.list, WM_SETREDRAW, 0, 0);
    message(data.list, LB_RESETCONTENT, 0, 0);
    for (i, (label, details)) in data.rows.iter().enumerate() {
        if !label.to_lowercase().contains(&query) {
            continue;
        }
        let title = wide(format!("{label}  ·  {details}"));
        let row = message(data.list, LB_ADDSTRING, 0, title.as_ptr() as isize);
        if row >= 0 {
            data.filtered.push(i);
        }
    }
    message(data.list, LB_SETCURSEL, 0, 0);
    message(data.list, WM_SETREDRAW, 1, 0);
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
            Some(data.list),
            Some(&client(data.list)),
            true,
        );
    }
}

unsafe extern "system" fn search_proc(hwnd: HWND, kind: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if kind == WM_INITDIALOG {
        let data = unsafe { &mut *(lp.0 as *mut Search<'_>) };
        set_text(
            hwnd,
            OsStr::new(tr!("搜索桌面格", "搜尋桌面格", "Search DeskGrid")),
        );
        data.edit = control(
            hwnd,
            w!("Edit"),
            OsStr::new(""),
            WS_TABSTOP.0 | ES_AUTOHSCROLL as u32,
            100,
            box_at(8, 9, 347, 17),
        );
        data.list = control(
            hwnd,
            w!("ListBox"),
            OsStr::new(""),
            WS_TABSTOP.0 | WS_VSCROLL.0 | LBS_NOTIFY as u32,
            101,
            box_at(8, 31, 347, 150),
        );
        button(
            hwnd,
            tr!("打开", "開啟", "Open"),
            IDOK.0 as usize,
            box_at(8, 187, 67, 17),
            true,
        );
        button(
            hwnd,
            tr!("在面板中定位", "在面板中定位", "Show in panel"),
            103,
            box_at(81, 187, 86, 17),
            false,
        );
        button(
            hwnd,
            tr!("关闭", "關閉", "Close"),
            IDCANCEL.0 as usize,
            box_at(288, 187, 67, 17),
            false,
        );
        filter(data);
        let edit = data.edit;
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lp.0);
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(edit));
        }
        return 0;
    }
    if kind != WM_COMMAND {
        return 0;
    }
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Search<'_> };
    if pointer.is_null() {
        return 0;
    }
    let data = unsafe { &mut *pointer };
    let id = wp.0 & 0xffff;
    let notification = (wp.0 >> 16) as u32;
    if id == IDCANCEL.0 as usize {
        unsafe {
            let _ = EndDialog(hwnd, 0);
        }
    } else if id == 100 && notification == EN_CHANGE {
        filter(data);
    } else if id == IDOK.0 as usize || id == 103 || (id == 101 && notification == LBN_DBLCLK) {
        if let Ok(row) = usize::try_from(message(data.list, LB_GETCURSEL, 0, 0))
            && let Some(&index) = data.filtered.get(row)
        {
            data.result = Some((index, id == 103));
            unsafe {
                let _ = EndDialog(hwnd, 1);
            }
        }
    } else {
        return 0;
    }
    1
}

pub fn search(owner: HWND, rows: &[(String, String)]) -> Option<(usize, bool)> {
    let mut data = Search {
        rows,
        edit: HWND::default(),
        list: HWND::default(),
        filtered: Vec::new(),
        result: None,
    };
    modal(
        owner,
        212,
        Some(search_proc),
        LPARAM(&mut data as *mut Search<'_> as isize),
    );
    data.result
}
