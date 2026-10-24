//! The little bar window the character sits on. It floats above everything,
//! lets clicks pass through, and never takes keyboard focus from the app you're
//! dictating into.

use crate::settings::Settings;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

pub const WIDTH: f64 = 380.0;
pub const HEIGHT: f64 = 300.0;
const LABEL: &str = "overlay";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
        .title("Baa bar")
        .theme(Some(tauri::Theme::Light))
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()?;
    w.set_ignore_cursor_events(true)?;
    #[cfg(windows)]
    win::no_activate(&w);
    Ok(())
}

/// Shows whichever pop-up the settings pick: the bar, or the character (buddy mode).
pub fn show(app: &AppHandle, s: &Settings) {
    if s.overlay_style == "buddy" {
        if s.buddy_place == "popup" {
            crate::buddy::place(app, s);
        }
        return crate::buddy::show(app);
    }
    show_bar(app, s)
}

/// Places the bar on the monitor under the mouse, then shows it without focus.
fn show_bar(app: &AppHandle, s: &Settings) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let cursor = app.cursor_position().ok();
    let monitors = app.available_monitors().unwrap_or_default();
    let mon = cursor
        .and_then(|c| {
            monitors.iter().find(|m| {
                let p = m.position();
                let sz = m.size();
                c.x >= p.x as f64 && c.x < (p.x + sz.width as i32) as f64 && c.y >= p.y as f64 && c.y < (p.y + sz.height as i32) as f64
            })
        })
        .cloned()
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = mon {
        let area = m.work_area();
        let scale = m.scale_factor();
        let (ww, wh) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32);
        let margin = (12.0 * scale) as i32;
        let (ax, ay, aw, ah) = (area.position.x, area.position.y, area.size.width as i32, area.size.height as i32);
        let (x, y) = match s.overlay_position.as_str() {
            "top" => (ax + (aw - ww) / 2, ay + margin),
            "bottom_left" => (ax + margin, ay + ah - wh - margin),
            "bottom_right" => (ax + aw - ww - margin, ay + ah - wh - margin),
            "top_left" => (ax + margin, ay + margin),
            "top_right" => (ax + aw - ww - margin, ay + margin),
            _ => (ax + (aw - ww) / 2, ay + ah - wh - margin),
        };
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    #[cfg(windows)]
    win::show_no_activate(&w);
    #[cfg(not(windows))]
    let _ = w.show();
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
    // an always-there buddy stays; a pop-up buddy goes away like the bar
    let st = app.state::<std::sync::Arc<crate::AppState>>();
    let s = st.settings.get();
    if s.overlay_style != "buddy" || s.buddy_place == "popup" {
        crate::buddy::hide(app);
    }
}

#[cfg(windows)]
pub use win::{no_activate, show_no_activate};

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    fn hwnd(w: &tauri::WebviewWindow) -> Option<HWND> {
        // tauri may link a different `windows` crate version, so go through the raw pointer
        w.hwnd().ok().map(|h| HWND(h.0 as _))
    }

    pub fn no_activate(w: &tauri::WebviewWindow) {
        if let Some(h) = hwnd(w) {
            unsafe {
                let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
                SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
            }
        }
    }

    pub fn show_no_activate(w: &tauri::WebviewWindow) {
        if let Some(h) = hwnd(w) {
            unsafe {
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        } else {
            let _ = w.show();
        }
    }
}
