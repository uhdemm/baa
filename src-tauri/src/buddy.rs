//! Buddy mode: the character itself, with a speech bubble over its head, instead
//! of the bar. Three ways to have it:
//! - "taskbar": always there, sitting on top of the taskbar (slide it along by dragging);
//! - "popup": pops up only while you dictate, where the bar would be;
//! - "free": always there, wherever you drag it.
//! The window is see-through and lets clicks pass, except over the character
//! itself (so you can grab it): a small timer checks where the mouse is.

use crate::settings::Settings;
use parking_lot::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "buddy";
const WIDTH: f64 = 300.0;

/// The character's box inside the window (logical px), sent by the page.
static HIT: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);

fn height(s: &Settings) -> f64 {
    // room for the bubble above the character
    match s.buddy_size.as_str() {
        "small" => 150.0,
        "large" => 220.0,
        _ => 170.0,
    }
}

pub fn create(app: &AppHandle, s: &Settings) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("buddy.html".into()))
        .title("Baa buddy")
        .theme(Some(tauri::Theme::Light))
        .inner_size(WIDTH, height(s))
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
    crate::overlay::no_activate(&w);
    // let the mouse through everywhere except over the character
    let app2 = app.clone();
    std::thread::spawn(move || {
        let mut grabbable = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(60));
            let Some(w) = app2.get_webview_window(LABEL) else { return };
            if !w.is_visible().unwrap_or(false) {
                continue;
            }
            let over = match (app2.cursor_position(), w.outer_position(), w.scale_factor(), *HIT.lock()) {
                (Ok(c), Ok(p), Ok(k), Some((x, y, bw, bh))) => {
                    let (cx, cy) = ((c.x - p.x as f64) / k, (c.y - p.y as f64) / k);
                    cx >= x && cx <= x + bw && cy >= y && cy <= y + bh
                }
                _ => false,
            };
            if over != grabbable {
                grabbable = over;
                let _ = w.set_ignore_cursor_events(!over);
            }
        }
    });
    Ok(())
}

pub fn set_hit(rect: Option<(f64, f64, f64, f64)>) {
    *HIT.lock() = rect;
}

/// Puts the window where the settings say, on the monitor it was on (or the main one).
pub fn place(app: &AppHandle, s: &Settings) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let mons = app.available_monitors().unwrap_or_default();
    let at = w.outer_position().ok();
    let mon = at
        .and_then(|p| {
            mons.iter().find(|m| {
                let (mp, ms) = (m.position(), m.size());
                p.x >= mp.x && p.x < mp.x + ms.width as i32 && p.y >= mp.y - 400 && p.y < mp.y + ms.height as i32
            })
        })
        .cloned()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(m) = mon else { return };
    let k = m.scale_factor();
    let (ww, wh) = ((WIDTH * k) as i32, (height(s) * k) as i32);
    let _ = w.set_size(PhysicalSize::new(ww as u32, wh as u32));
    let area = m.work_area();
    let (ax, ay, aw, ah) = (area.position.x, area.position.y, area.size.width as i32, area.size.height as i32);
    let fx = s.buddy_x.clamp(0.0, 1.0);
    let (x, y) = match s.buddy_place.as_str() {
        // the character's flat bottom edge sits right on the taskbar (the work area's bottom)
        "taskbar" => (ax + ((aw - ww) as f64 * fx) as i32, ay + ah - wh),
        "free" => (ax + ((aw - ww) as f64 * fx) as i32, ay + ((ah - wh) as f64 * s.buddy_y.clamp(0.0, 1.0)) as i32),
        _ => {
            let margin = (12.0 * k) as i32;
            match s.overlay_position.as_str() {
                "top" => (ax + (aw - ww) / 2, ay + margin),
                "bottom_left" => (ax + margin, ay + ah - wh),
                "bottom_right" => (ax + aw - ww - margin, ay + ah - wh),
                "top_left" => (ax + margin, ay + margin),
                "top_right" => (ax + aw - ww - margin, ay + margin),
                _ => (ax + (aw - ww) / 2, ay + ah - wh),
            }
        }
    };
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

/// Shows or hides the buddy to match the settings (called on start and on every change).
pub fn sync(app: &AppHandle, s: &Settings) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let always = s.overlay_style == "buddy" && s.buddy_place != "popup";
    place(app, s);
    if always {
        show(app);
    } else {
        let _ = w.hide();
    }
}

pub fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        #[cfg(windows)]
        crate::overlay::show_no_activate(&w);
        #[cfg(not(windows))]
        let _ = w.show();
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// After you drag it: remember where, as a fraction of the screen, and snap back
/// onto the taskbar in taskbar mode. Returns the updated settings.
pub fn moved(app: &AppHandle, s: &mut Settings) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let (Ok(p), Ok(sz)) = (w.outer_position(), w.outer_size()) else { return };
    let mons = app.available_monitors().unwrap_or_default();
    let cx = p.x + sz.width as i32 / 2;
    let Some(m) = mons.iter().find(|m| cx >= m.position().x && cx < m.position().x + m.size().width as i32).or(mons.first()) else { return };
    let area = m.work_area();
    let free_w = (area.size.width as i32 - sz.width as i32).max(1) as f64;
    let free_h = (area.size.height as i32 - sz.height as i32).max(1) as f64;
    s.buddy_x = ((p.x - area.position.x) as f64 / free_w).clamp(0.0, 1.0);
    if s.buddy_place == "free" {
        s.buddy_y = ((p.y - area.position.y) as f64 / free_h).clamp(0.0, 1.0);
    }
    if s.buddy_place != "popup" {
        place(app, s); // taskbar mode: back down onto the taskbar
    }
}
