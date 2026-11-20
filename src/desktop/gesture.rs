use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::layout::Bounds;

use super::platform::*;
use super::{Action, dispatch, theme};

pub const UPDATE: u32 = WM_APP + 4;
pub const REPLAY: u32 = WM_APP + 5;
const CLASS: windows::core::PCWSTR = w!("DeskGrid.PageSelection");
const INPUT_TAG: usize = 0x4453_4b47;
const KEY: COLORREF = theme::rgb(1, 2, 3);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

#[derive(Default)]
struct State {
    receiver: HWND,
    overlay: HWND,
    explorer: Explorer,
    icons: Option<Rc<[RECT]>>,
    quick_hide: bool,
    last_click: Option<(POINT, u32)>,
    drawing: Option<Selection>,
    replay: Option<POINT>,
    update_pending: bool,
}

struct Selection {
    start: POINT,
    end: POINT,
    work: Bounds,
    threshold: i32,
    cancelled: bool,
}

impl Selection {
    fn bounds(&self) -> Option<Bounds> {
        let x = self.end.x.clamp(self.work.left, self.work.right());
        let y = self.end.y.clamp(self.work.top, self.work.bottom());
        let width = (self.start.x - x).abs();
        let height = (self.start.y - y).abs();
        (!self.cancelled && width >= self.threshold && height >= self.threshold).then_some(Bounds {
            left: self.start.x.min(x),
            top: self.start.y.min(y),
            width,
            height,
        })
    }
}

pub struct DesktopInput {
    hook: HHOOK,
    overlay: HWND,
}

impl DesktopInput {
    pub fn new(receiver: HWND) -> windows::core::Result<Self> {
        register(CLASS, Some(overlay_proc), WNDCLASS_STYLES(0))?;
        let overlay = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                CLASS,
                None,
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
        let hook = unsafe {
            SetLayeredWindowAttributes(overlay, KEY, 228, LWA_ALPHA | LWA_COLORKEY)
                .and_then(|_| SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), Some(instance()), 0))
        };
        match hook {
            Ok(hook) => {
                STATE.with(|state| {
                    *state.borrow_mut() = State {
                        receiver,
                        overlay,
                        ..State::default()
                    };
                });
                Ok(Self { hook, overlay })
            }
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(overlay);
                }
                Err(error)
            }
        }
    }

    pub fn update(&self, explorer: Explorer, icons: Option<Rc<[RECT]>>, quick_hide: bool) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.explorer != explorer || state.quick_hide != quick_hide {
                state.last_click = None;
            }
            if state.explorer != explorer
                && let Some(selection) = &mut state.drawing
            {
                selection.cancelled = true;
            }
            state.explorer = explorer;
            state.icons = icons;
            state.quick_hide = quick_hide;
        });
    }
}

impl Drop for DesktopInput {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.hook);
            let _ = DestroyWindow(self.overlay);
        }
        STATE.with(|state| *state.borrow_mut() = State::default());
    }
}

impl State {
    fn blank(&self, point: POINT) -> bool {
        self.explorer.blank(point, self.icons.as_deref())
    }

    fn post(&self, message: u32) -> bool {
        unsafe { PostMessageW(Some(self.receiver), message, WPARAM(0), LPARAM(0)).is_ok() }
    }

    fn repaint(&mut self) {
        if !self.update_pending {
            self.update_pending = self.post(UPDATE);
        }
    }

    fn mouse(&mut self, message: u32, event: &MSLLHOOKSTRUCT) -> bool {
        let point = event.pt;
        match message {
            WM_RBUTTONDOWN
                if self.blank(point) && unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } >= 0 =>
            {
                let work = work_area(None, Some(point));
                if !contains(rect(work.left, work.top, work.width, work.height), point) {
                    return false;
                }
                self.last_click = None;
                self.drawing = Some(Selection {
                    start: point,
                    end: point,
                    work,
                    threshold: px(32, monitor_dpi(point)),
                    cancelled: false,
                });
                self.repaint();
                true
            }
            WM_MOUSEMOVE if self.drawing.is_some() => {
                self.drawing.as_mut().unwrap().end = point;
                self.repaint();
                false
            }
            WM_RBUTTONUP if self.drawing.is_some() => {
                let mut selection = self.drawing.take().unwrap();
                selection.end = point;
                selection.cancelled |= escape_down();
                self.repaint();
                if let Some(bounds) = selection.bounds() {
                    dispatch(Action::DrawGroup(bounds));
                } else if !selection.cancelled
                    && (point.x - selection.start.x).abs() < selection.threshold
                    && (point.y - selection.start.y).abs() < selection.threshold
                {
                    self.replay = Some(point);
                    self.post(REPLAY);
                }
                true
            }
            WM_LBUTTONDOWN if self.drawing.is_some() => {
                self.drawing.as_mut().unwrap().cancelled = true;
                self.repaint();
                false
            }
            WM_LBUTTONDOWN if self.quick_hide => {
                if !self.blank(point) {
                    self.last_click = None;
                    return false;
                }
                let double = self.last_click.is_some_and(|(last, time)| unsafe {
                    event.time.wrapping_sub(time) <= GetDoubleClickTime()
                        && (point.x - last.x).abs() <= GetSystemMetrics(SM_CXDOUBLECLK) / 2
                        && (point.y - last.y).abs() <= GetSystemMetrics(SM_CYDOUBLECLK) / 2
                });
                self.last_click = (!double).then_some((point, event.time));
                if double {
                    self.post(super::QUICK_HIDE);
                }
                false
            }
            _ => false,
        }
    }
}

unsafe extern "system" fn mouse(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = unsafe { &*(data.0 as *const MSLLHOOKSTRUCT) };
        if event.dwExtraInfo != INPUT_TAG
            && STATE.with(|state| state.borrow_mut().mouse(message.0 as u32, event))
        {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, message, data) }
}

fn escape_down() -> bool {
    (unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) }) < 0
}

#[cfg(test)]
#[test]
fn selection_is_clamped_to_the_starting_monitor() {
    let mut selection = Selection {
        start: POINT { x: -250, y: 300 },
        end: POINT { x: -1000, y: 50 },
        work: Bounds {
            left: -900,
            top: 100,
            width: 900,
            height: 600,
        },
        threshold: 32,
        cancelled: false,
    };
    assert_eq!(
        selection.bounds(),
        Some(Bounds {
            left: -900,
            top: 100,
            width: 650,
            height: 200
        }),
    );
    selection.cancelled = true;
    assert_eq!(selection.bounds(), None);
    selection.cancelled = false;
    selection.end = POINT { x: -200, y: 310 };
    assert_eq!(selection.bounds(), None);
}

#[cfg(test)]
#[test]
#[ignore = "requires an exposed Windows desktop"]
fn right_button_events_distinguish_a_click_from_drawing_a_group() {
    let _ole = super::Ole::new().unwrap();
    let explorer = explorer();
    let icons: Rc<[RECT]> = explorer.icons().unwrap().into();
    let work = work_area(None, None);
    let point = (1..16)
        .flat_map(|row| {
            (1..16).map(move |col| POINT {
                x: work.left + col * work.width / 17,
                y: work.top + row * work.height / 17,
            })
        })
        .find(|&point| explorer.blank(point, Some(&icons)))
        .expect("no accessible blank desktop area");
    let mut state = State {
        explorer,
        icons: Some(icons),
        ..State::default()
    };
    let mut mouse = MSLLHOOKSTRUCT {
        pt: point,
        ..MSLLHOOKSTRUCT::default()
    };
    assert!(state.mouse(WM_RBUTTONDOWN, &mouse));
    assert!(state.mouse(WM_RBUTTONUP, &mouse));
    assert_eq!(state.replay.take(), Some(point));

    assert!(state.mouse(WM_RBUTTONDOWN, &mouse));
    mouse.pt.x += (work.right() - point.x).min(240);
    mouse.pt.y += (work.bottom() - point.y).min(180);
    state.mouse(WM_MOUSEMOVE, &mouse);
    let expected = state.drawing.as_ref().unwrap().bounds().unwrap();
    assert!(state.mouse(WM_RBUTTONUP, &mouse));
    assert!(state.replay.is_none());
    assert!(super::ACTIONS.with(|actions| matches!(
        actions.borrow_mut().pop_front(), Some(Action::DrawGroup(bounds)) if bounds == expected
    )));
}

pub fn replay_click() {
    let replay = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let end = state.replay.take()?;
        let point = cursor();
        ((point.x - end.x).abs() <= 3 && (point.y - end.y).abs() <= 3 && state.blank(point))
            .then_some(())
    });
    if replay.is_some() {
        let inputs = [MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP].map(|flags| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    dwExtraInfo: INPUT_TAG,
                    ..Default::default()
                },
            },
        });
        unsafe {
            SendInput(&inputs, size_of::<INPUT>() as i32);
        }
    }
}

pub fn update_preview() {
    let (overlay, bounds, drawing) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.update_pending = false;
        if let Some(selection) = &mut state.drawing {
            selection.cancelled |= escape_down();
        }
        (
            state.overlay,
            state.drawing.as_ref().and_then(Selection::bounds),
            state
                .drawing
                .as_ref()
                .is_some_and(|selection| !selection.cancelled),
        )
    });
    unsafe {
        if let Some(bounds) = bounds {
            let _ = SetWindowPos(
                overlay,
                Some(HWND_TOPMOST),
                bounds.left,
                bounds.top,
                bounds.width,
                bounds.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(Some(overlay), None, false);
        } else {
            let _ = ShowWindow(overlay, SW_HIDE);
        }
        if drawing {
            SetTimer(Some(overlay), 1, 16, None);
        } else {
            let _ = KillTimer(Some(overlay), 1);
        }
    }
}

unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match message {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_TIMER => {
            if escape_down() {
                update_preview();
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut painting = PAINTSTRUCT::default();
            unsafe {
                let canvas = theme::Canvas {
                    dc: BeginPaint(hwnd, &mut painting),
                    dpi: dpi(hwnd),
                };
                let area = client(hwnd);
                canvas.fill(area, KEY);
                let edge = canvas.unit(2);
                for line in [
                    rect(0, 0, area.right, edge),
                    rect(0, area.bottom - edge, area.right, edge),
                    rect(0, 0, edge, area.bottom),
                    rect(area.right - edge, 0, edge, area.bottom),
                ] {
                    canvas.fill(line, theme::ACCENT);
                }
                let _ = EndPaint(hwnd, &painting);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wp, lp) },
    }
}
