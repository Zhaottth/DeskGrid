use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, w};

use super::platform::{px, rect, wide};

pub const BG: COLORREF = rgb(17, 22, 32);
pub const RAIL: COLORREF = rgb(22, 28, 41);
pub const CARD: COLORREF = rgb(27, 35, 51);
pub const WELL: COLORREF = rgb(15, 21, 32);
pub const EDGE: COLORREF = rgb(47, 60, 81);
pub const SELECTED: COLORREF = rgb(38, 53, 84);
pub const TEXT: COLORREF = rgb(242, 246, 255);
pub const MUTED: COLORREF = rgb(168, 183, 208);
pub const ACCENT: COLORREF = rgb(165, 186, 255);

pub const fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | g << 8 | b << 16)
}

pub fn mix(base: COLORREF, foreground: COLORREF, amount: u32) -> COLORREF {
    let blend = |shift: u32| {
        (((base.0 >> shift) & 255u32) * (100 - amount)
            + ((foreground.0 >> shift) & 255u32) * amount)
            / 100u32
    };
    rgb(blend(0), blend(8), blend(16))
}

pub fn frame(hwnd: HWND) {
    unsafe {
        let dark = BOOL::from(true);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark as *const BOOL).cast(),
            4,
        );
        for (attribute, color) in [
            (DWMWA_CAPTION_COLOR, RAIL),
            (DWMWA_TEXT_COLOR, TEXT),
            (DWMWA_BORDER_COLOR, EDGE),
        ] {
            let _ = DwmSetWindowAttribute(hwnd, attribute, (&color as *const COLORREF).cast(), 4);
        }
    }
}

pub fn title(hwnd: HWND) -> String {
    unsafe {
        let mut text = vec![0; GetWindowTextLengthW(hwnd).max(0) as usize + 1];
        let len = GetWindowTextW(hwnd, &mut text).max(0) as usize;
        String::from_utf16_lossy(&text[..len])
    }
}

pub struct Canvas {
    pub dc: HDC,
    pub dpi: i32,
}

impl Canvas {
    pub fn unit(&self, n: i32) -> i32 {
        px(n, self.dpi)
    }

    pub fn at(&self, x: i32, y: i32, width: i32, height: i32) -> RECT {
        rect(
            self.unit(x),
            self.unit(y),
            self.unit(width),
            self.unit(height),
        )
    }

    pub fn fill(&self, area: RECT, color: COLORREF) {
        unsafe {
            let brush = HBRUSH(GetStockObject(DC_BRUSH).0);
            SetDCBrushColor(self.dc, color);
            FillRect(self.dc, &area, brush);
        }
    }

    pub fn card(&self, area: RECT, fill: COLORREF, border: COLORREF, radius: i32) {
        unsafe {
            let brush = SelectObject(self.dc, GetStockObject(DC_BRUSH));
            let pen = SelectObject(self.dc, GetStockObject(DC_PEN));
            SetDCBrushColor(self.dc, fill);
            SetDCPenColor(self.dc, border);
            let radius = self.unit(2 * radius);
            let _ = RoundRect(
                self.dc,
                area.left,
                area.top,
                area.right,
                area.bottom,
                radius,
                radius,
            );
            SelectObject(self.dc, brush);
            SelectObject(self.dc, pen);
        }
    }

    pub fn text(
        &self,
        text: &str,
        mut area: RECT,
        color: COLORREF,
        size: i32,
        weight: i32,
        align: DRAW_TEXT_FORMAT,
    ) {
        let mut text = wide(text);
        text.pop();
        unsafe {
            let font = CreateFontW(
                -self.unit(size),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Microsoft YaHei UI"),
            );
            let before = SelectObject(self.dc, font.into());
            let mode = SetBkMode(self.dc, TRANSPARENT);
            let previous = SetTextColor(self.dc, color);
            DrawTextW(
                self.dc,
                &mut text,
                &mut area,
                DT_NOPREFIX | DT_END_ELLIPSIS | DT_VCENTER | DT_SINGLELINE | align,
            );
            SetTextColor(self.dc, previous);
            SetBkMode(self.dc, BACKGROUND_MODE(mode as u32));
            SelectObject(self.dc, before);
            let _ = DeleteObject(font.into());
        }
    }

    pub fn icon(&self, icon: HICON, x: i32, y: i32, size: i32) {
        unsafe {
            let _ = DrawIconEx(self.dc, x, y, icon, size, size, 0, None, DI_NORMAL);
        }
    }
}

#[derive(Clone, Copy)]
pub enum Button {
    Primary,
    Outline,
    Navigation,
}

pub fn button(draw: &DRAWITEMSTRUCT, dpi: i32, appearance: Button, background: COLORREF) {
    let c = Canvas { dc: draw.hDC, dpi };
    let area = draw.rcItem;
    let saved = unsafe { SaveDC(c.dc) };
    c.fill(area, background);
    let focused = draw.itemState.0 & ODS_FOCUS.0 != 0 && draw.itemState.0 & ODS_NOFOCUSRECT.0 == 0;
    let pressed = draw.itemState.0 & ODS_SELECTED.0 != 0;
    let hovered = draw.itemState.0 & ODS_HOTLIGHT.0 != 0;
    let disabled = draw.itemState.0 & ODS_DISABLED.0 != 0;
    let (mut fill, mut border, mut ink) = match appearance {
        Button::Primary => (ACCENT, ACCENT, rgb(17, 28, 58)),
        Button::Outline => (CARD, EDGE, TEXT),
        Button::Navigation => (background, background, MUTED),
    };
    if disabled {
        fill = mix(background, fill, 40);
        border = mix(background, border, 50);
        ink = MUTED;
    } else if pressed || hovered {
        fill = mix(fill, TEXT, if pressed { 20 } else { 10 });
    }
    if focused {
        border = ACCENT;
    }
    let nav = matches!(appearance, Button::Navigation);
    c.card(area, fill, border, 7);
    let mut label = area;
    if nav {
        label.left += c.unit(16);
    }
    c.text(
        &title(draw.hwndItem),
        label,
        ink,
        14,
        600,
        if nav { DT_LEFT } else { DT_CENTER },
    );
    unsafe {
        let _ = RestoreDC(c.dc, saved);
    }
}
