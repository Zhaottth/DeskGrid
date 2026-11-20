use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::SystemTime;

use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx, HICON};

use crate::directory::Entry;

use super::platform::{Icon, item_icon};

#[derive(Clone, PartialEq, Eq, Hash)]
struct ImageKey(String, Option<SystemTime>);

struct Worker {
    jobs: mpsc::SyncSender<(ImageKey, Entry)>,
    results: mpsc::Receiver<(ImageKey, Option<Icon>)>,
    pending: HashSet<ImageKey>,
    actual: HashMap<ImageKey, Option<Icon>>,
}

impl Worker {
    fn new() -> Option<Self> {
        let (jobs, requests) = mpsc::sync_channel::<(ImageKey, Entry)>(64);
        let (sender, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("DeskGrid Shell icons".into())
            .spawn(move || {
                if unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_err() {
                    return;
                }
                while let Ok((key, entry)) = requests.recv() {
                    let icon = item_icon(&entry, true);
                    if sender.send((key, icon)).is_err() {
                        break;
                    }
                }
                unsafe {
                    CoUninitialize();
                }
            })
            .ok()?;
        Some(Self {
            jobs,
            results,
            pending: HashSet::new(),
            actual: HashMap::new(),
        })
    }

    fn icon(&mut self, entry: &Entry) -> Option<HICON> {
        while let Ok((key, icon)) = self.results.try_recv() {
            self.pending.remove(&key);
            if self.actual.len() >= 512 {
                self.actual.clear();
            }
            self.actual.insert(key, icon);
        }
        if !matches!(entry.extension.as_str(), "lnk" | "url" | "exe" | "ico") {
            return None;
        }
        let key = ImageKey(entry.key.clone(), entry.modified);
        if let Some(icon) = self.actual.get(&key) {
            return icon.as_ref().map(Icon::handle);
        }
        if !self.pending.contains(&key) && self.jobs.try_send((key.clone(), entry.clone())).is_ok()
        {
            self.pending.insert(key);
        }
        None
    }
}

struct Cache {
    generic: HashMap<String, Option<Icon>>,
    worker: Option<Worker>,
}

thread_local! {
    static ICONS: RefCell<Cache> = RefCell::new(Cache { generic: HashMap::new(), worker: Worker::new() });
}

pub fn draw(dc: HDC, entry: &Entry, x: i32, y: i32, size: i32, tint: Option<COLORREF>) {
    ICONS.with(|icons| {
        let Ok(mut cache) = icons.try_borrow_mut() else {
            return;
        };
        let actual = cache.worker.as_mut().and_then(|worker| worker.icon(entry));
        let icon = actual.or_else(|| {
            let key = if entry.is_recycle() || entry.is_namespace() {
                entry.key.clone()
            } else if entry.folder {
                "folder".into()
            } else {
                format!(".{}", entry.extension)
            };
            if cache.generic.len() >= 512 && !cache.generic.contains_key(&key) {
                cache.generic.clear();
            }
            cache
                .generic
                .entry(key)
                .or_insert_with(|| item_icon(entry, false))
                .as_ref()
                .map(Icon::handle)
        });
        if let Some(icon) = icon
            && !tint.is_some_and(|color| tinted(dc, icon, x, y, size, color))
        {
            unsafe {
                let _ = DrawIconEx(dc, x, y, icon, size, size, 0, None, DI_NORMAL);
            }
        }
    });
}

fn tinted(dc: HDC, icon: HICON, x: i32, y: i32, size: i32, color: COLORREF) -> bool {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = std::ptr::null_mut();
    unsafe {
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut pixels, None, 0)
        else {
            return false;
        };
        let buffer = CreateCompatibleDC(Some(dc));
        if buffer.is_invalid() {
            let _ = DeleteObject(bitmap.into());
            return false;
        }
        let old = SelectObject(buffer, bitmap.into());
        let pixel_count = (size * size) as usize;
        std::ptr::write_bytes(pixels.cast::<[u8; 4]>(), 0, pixel_count);
        let _ = DrawIconEx(buffer, 0, 0, icon, size, size, 0, None, DI_NORMAL);
        let _ = GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(pixels.cast::<[u8; 4]>(), pixel_count);
        let mut has_alpha = false;
        let [red, green, blue, _] = color.0.to_le_bytes();
        for pixel in pixels {
            let alpha = u16::from(pixel[3]);
            has_alpha |= alpha > 0;
            for (component, target) in pixel[..3].iter_mut().zip([blue, green, red]) {
                *component =
                    ((u16::from(*component) * 2 + u16::from(target) * alpha / 255) / 3) as u8;
            }
        }
        let painted = has_alpha
            && AlphaBlend(
                dc,
                x,
                y,
                size,
                size,
                buffer,
                0,
                0,
                size,
                size,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                },
            )
            .as_bool();
        SelectObject(buffer, old);
        let _ = DeleteDC(buffer);
        let _ = DeleteObject(bitmap.into());
        painted
    }
}
