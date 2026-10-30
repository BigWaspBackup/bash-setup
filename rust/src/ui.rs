//! egui / Win32 helpers shared by both executables.

use std::{
    io::Cursor,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

use eframe::{egui, NativeOptions};
use egui::{vec2, IconData};
use image::ImageReader;
use windows::{
    core::{BOOL, HSTRING},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT},
        System::Threading::GetCurrentProcessId,
        UI::{
            Shell::{DefSubclassProc, SetWindowSubclass, ShellExecuteW},
            WindowsAndMessaging::{
                EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, MessageBoxW,
                IDNO, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO,
                MB_YESNOCANCEL,
                MESSAGEBOX_STYLE, SW_SHOWNORMAL, WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE,
                WM_MOVING, WM_PAINT, WM_SIZING,
            },
        },
    },
};

use crate::{log::log_line, ICON_BYTES};

pub const WINDOW_TITLE: &str = "BASH Launcher 2.0";

pub fn load_icon() -> Result<IconData, String> {
    let img = ImageReader::with_format(Cursor::new(ICON_BYTES), image::ImageFormat::Png)
        .decode()
        .map_err(|e| format!("Failed to decode embedded icon image: {}", e))?;
    // The source art is 705x690. CreateIcon rejects or crashes on icons that
    // large, which aborts startup before any window exists.
    let img = img.resize(256, 256, image::imageops::FilterType::Triangle);
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

/// Native options. Vsync stays on: with it off, Windows paints the mouse-move
/// region from an unfinished frame and the window flashes black rectangles.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn from_saved(value: &str) -> Self {
        if value.eq_ignore_ascii_case("light") {
            Self::Light
        } else {
            Self::Dark
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }

    pub fn visuals(self) -> egui::Visuals {
        match self {
            Self::Dark => egui::Visuals::dark(),
            Self::Light => egui::Visuals::light(),
        }
    }
}

pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    if ctx.style().visuals.dark_mode != theme.is_dark() {
        ctx.set_visuals(theme.visuals());
    }
}

/// Small "?" in a circle. Hover shows the tip.
pub fn help_tip(ui: &mut egui::Ui, tip: &str) {
    if tip.is_empty() {
        return;
    }
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let dark = ui.visuals().dark_mode;
    let fill = if response.hovered() {
        if dark {
            egui::Color32::from_rgb(70, 78, 90)
        } else {
            egui::Color32::from_rgb(200, 208, 220)
        }
    } else if dark {
        egui::Color32::from_rgb(48, 52, 60)
    } else {
        egui::Color32::from_rgb(220, 224, 230)
    };
    let stroke = if dark {
        egui::Color32::from_rgb(150, 158, 170)
    } else {
        egui::Color32::from_rgb(90, 98, 110)
    };
    let text = if dark {
        egui::Color32::from_rgb(210, 216, 224)
    } else {
        egui::Color32::from_rgb(50, 56, 66)
    };
    ui.painter().circle_filled(rect.center(), 7.5, fill);
    ui.painter().circle_stroke(rect.center(), 7.5, egui::Stroke::new(1.0_f32, stroke));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "?",
        egui::FontId::proportional(11.0),
        text,
    );
    let _ = response.on_hover_text(tip);
}

/// Card and row fills. Dark mode keeps the current panels. Light mode uses a light panel.
pub fn panel_fill(visuals: &egui::Visuals) -> egui::Color32 {
    if visuals.dark_mode {
        egui::Color32::from_rgb(32, 36, 42)
    } else {
        egui::Color32::from_rgb(236, 237, 240)
    }
}

pub fn row_fill(visuals: &egui::Visuals, selected: bool, hovered: bool) -> egui::Color32 {
    if visuals.dark_mode {
        if selected {
            egui::Color32::from_rgb(46, 62, 78)
        } else if hovered {
            egui::Color32::from_rgb(40, 44, 52)
        } else {
            egui::Color32::from_rgb(30, 32, 36)
        }
    } else if selected {
        egui::Color32::from_rgb(210, 226, 240)
    } else if hovered {
        egui::Color32::from_rgb(228, 230, 234)
    } else {
        egui::Color32::from_rgb(248, 248, 246)
    }
}

pub fn chip_fill(visuals: &egui::Visuals, selected: bool, hovered: bool) -> egui::Color32 {
    if visuals.dark_mode {
        if selected {
            egui::Color32::from_rgb(58, 46, 20)
        } else if hovered {
            egui::Color32::from_rgb(46, 46, 52)
        } else {
            egui::Color32::from_rgb(32, 30, 28)
        }
    } else if selected {
        egui::Color32::from_rgb(232, 214, 170)
    } else if hovered {
        egui::Color32::from_rgb(226, 228, 232)
    } else {
        egui::Color32::from_rgb(236, 237, 240)
    }
}

pub fn native_options(width: f32, height: f32) -> NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(vec2(width, height))
        .with_min_inner_size(vec2(width, height));
    match load_icon() {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(err) => log_line(&format!("icon skipped: {err}")),
    }
    NativeOptions {
        viewport,
        vsync: true,
        centered: true,
        ..Default::default()
    }
}

pub fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = ImageReader::with_format(Cursor::new(ICON_BYTES), image::ImageFormat::Png)
        .decode()
        .ok()?;
    let rgba = img.into_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture("bash_logo", color_image, Default::default()))
}

pub fn show_logo(ui: &mut egui::Ui, logo: &Option<egui::TextureHandle>, max_size: f32) {
    if let Some(logo) = logo {
        ui.add(egui::Image::new(logo).max_size(vec2(max_size, max_size)));
        ui.add_space(8.0);
    }
}

/// 0 idle, 1 moving, 2 resizing.
static DRAG_MODE: AtomicU8 = AtomicU8::new(0);
static DRAG_HOOKED: AtomicBool = AtomicBool::new(false);
static DRAG_HOOK_LOGGED: AtomicBool = AtomicBool::new(false);

/// Dragging a glow window repaints inside Windows' move loop, and vsync makes
/// that paint block the thread. The window then lags the cursor and the gap
/// flashes black. While the window is only being moved, keep the last frame
/// and let Windows slide it. Resizes still repaint, so the edges stay filled.
pub fn install_smooth_drag() {
    if DRAG_HOOKED.load(Ordering::Relaxed) {
        return;
    }
    let Some(hwnd) = find_our_window() else {
        return;
    };
    let ok = unsafe { SetWindowSubclass(hwnd, Some(drag_subclass), 1, 0) };
    if ok.as_bool() {
        DRAG_HOOKED.store(true, Ordering::Relaxed);
        return;
    }
    if !DRAG_HOOK_LOGGED.swap(true, Ordering::Relaxed) {
        log_line("smooth drag hook was not installed");
    }
}

struct FindHwnd {
    pid: u32,
    found: HWND,
}

unsafe extern "system" fn find_our_window_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if lparam.0 == 0 {
        return BOOL(1);
    }
    let find = &mut *(lparam.0 as *mut FindHwnd);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == find.pid && IsWindowVisible(hwnd).as_bool() {
        let mut buf = [0u16; 64];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n > 0 {
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            if title == WINDOW_TITLE {
                find.found = hwnd;
            }
        }
    }
    BOOL(1)
}

fn find_our_window() -> Option<HWND> {
    let mut find = FindHwnd {
        pid: unsafe { GetCurrentProcessId() },
        found: HWND(std::ptr::null_mut()),
    };
    let param = LPARAM(&mut find as *mut FindHwnd as isize);
    if unsafe { EnumWindows(Some(find_our_window_cb), param) }.is_err() {
        return None;
    }
    if find.found.0.is_null() {
        None
    } else {
        Some(find.found)
    }
}

unsafe extern "system" fn drag_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    // A partial erase is what leaves a black rectangle under the cursor.
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    if msg == WM_ENTERSIZEMOVE {
        DRAG_MODE.store(1, Ordering::Relaxed);
    } else if msg == WM_SIZING {
        DRAG_MODE.store(2, Ordering::Relaxed);
    } else if msg == WM_MOVING && DRAG_MODE.load(Ordering::Relaxed) != 2 {
        DRAG_MODE.store(1, Ordering::Relaxed);
    } else if msg == WM_EXITSIZEMOVE {
        DRAG_MODE.store(0, Ordering::Relaxed);
        let result = DefSubclassProc(hwnd, msg, wparam, lparam);
        let _ = InvalidateRect(Some(hwnd), None, false);
        return result;
    } else if msg == WM_PAINT && DRAG_MODE.load(Ordering::Relaxed) == 1 {
        let mut paint = PAINTSTRUCT::default();
        let _hdc = BeginPaint(hwnd, &mut paint);
        let _ = EndPaint(hwnd, &paint);
        return LRESULT(0);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

pub fn message_box(message: &str, style: MESSAGEBOX_STYLE) -> i32 {
    log_line(message);
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from(WINDOW_TITLE),
            style,
        )
        .0
    }
}

pub fn show_error(message: &str) {
    message_box(message, MB_OK | MB_ICONERROR);
}

pub fn show_info(message: &str) {
    message_box(message, MB_OK | MB_ICONINFORMATION);
}

pub fn ask_yes_no(message: &str) -> bool {
    message_box(message, MB_YESNO | MB_ICONQUESTION) == IDYES.0
}

pub enum SaveChoice {
    Yes,
    No,
    Cancel,
}

/// Yes saves, No closes without saving, Cancel stays on the window.
pub fn ask_save_changes(message: &str) -> SaveChoice {
    match message_box(message, MB_YESNOCANCEL | MB_ICONQUESTION) {
        result if result == IDYES.0 => SaveChoice::Yes,
        result if result == IDNO.0 => SaveChoice::No,
        _ => SaveChoice::Cancel,
    }
}

pub fn open_url(url: &str) -> bool {
    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("open"),
            &HSTRING::from(url),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    result.0 as isize > 32
}

/// Open a folder or file in Explorer. Missing folders are created. A missing
/// file opens its parent folder instead.
pub fn open_path(path: &std::path::Path) -> bool {
    let target = if path.exists() {
        path.to_path_buf()
    } else if path.extension().is_some() {
        path.parent()
            .map(|parent| parent.to_path_buf())
            .unwrap_or_else(|| path.to_path_buf())
    } else {
        let _ = std::fs::create_dir_all(path);
        path.to_path_buf()
    };
    if !target.exists() {
        return false;
    }
    open_url(&target.display().to_string())
}
