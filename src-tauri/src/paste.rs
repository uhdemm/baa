//! Puts text into whatever app has focus: clipboard + Ctrl+V, then optionally
//! presses Enter ("hits send") and puts the user's old clipboard back.

use crate::settings::AutoSend;
use enigo::{Direction, Enigo, Key, Keyboard};
use std::thread::sleep;
use std::time::Duration;

pub fn paste(text: &str, send: AutoSend, restore_clipboard: bool) -> anyhow::Result<()> {
    let mut cb = arboard::Clipboard::new()?;
    let previous = if restore_clipboard { cb.get_text().ok() } else { None };
    cb.set_text(text.to_string())?;
    sleep(Duration::from_millis(40));

    let mut kb = Enigo::new(&enigo::Settings::default())?;
    // Make sure no modifier from the shortcut is still logically down.
    for k in [Key::Shift, Key::Alt, Key::Meta] {
        let _ = kb.key(k, Direction::Release);
    }
    kb.key(Key::Control, Direction::Press)?;
    kb.key(Key::Unicode('v'), Direction::Click)?;
    kb.key(Key::Control, Direction::Release)?;

    if send != AutoSend::Off {
        sleep(Duration::from_millis(120)); // let the app take the paste first
        if send == AutoSend::CtrlEnter {
            kb.key(Key::Control, Direction::Press)?;
            kb.key(Key::Return, Direction::Click)?;
            kb.key(Key::Control, Direction::Release)?;
        } else {
            kb.key(Key::Return, Direction::Click)?;
        }
    }

    if let Some(prev) = previous {
        // Apps read the clipboard asynchronously after Ctrl+V; give them time.
        sleep(Duration::from_millis(350));
        let _ = cb.set_text(prev);
    }
    Ok(())
}

/// Exe name of the app that has keyboard focus, lowercased (e.g. "discord.exe").
#[cfg(windows)]
pub fn focused_app() -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        full.rsplit('\\').next().map(|s| s.to_lowercase())
    }
}

#[cfg(not(windows))]
pub fn focused_app() -> Option<String> {
    None
}
