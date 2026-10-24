//! Puts text into whatever app has focus: clipboard + Ctrl+V, then optionally
//! presses Enter ("hits send") and puts the user's old clipboard back.

use crate::settings::AutoSend;
use enigo::{Direction, Enigo, Key, Keyboard};
use std::thread::sleep;
use std::time::Duration;

pub fn paste(text: &str, send: AutoSend, restore_clipboard: bool) -> anyhow::Result<()> {
    paste_with_html(text, None, send, restore_clipboard)
}

/// Like `paste`, but also puts an HTML version on the clipboard so rich-text apps
/// get real paragraphs and bullet lists (plain-text apps still get the text).
pub fn paste_with_html(text: &str, html: Option<&str>, send: AutoSend, restore_clipboard: bool) -> anyhow::Result<()> {
    let mut cb = arboard::Clipboard::new()?;
    let previous = if restore_clipboard { cb.get_text().ok() } else { None };
    match html {
        Some(h) => cb.set_html(h, Some(text))?,
        None => cb.set_text(text.to_string())?,
    }
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

/// Copies the focused app's selection (Ctrl+C) without losing the user's clipboard.
/// Returns None when nothing is selected.
pub fn copy_selection() -> anyhow::Result<Option<String>> {
    let mut cb = arboard::Clipboard::new()?;
    let previous = cb.get_text().ok();
    let marker = format!("\u{2063}baa-{}", std::process::id());
    cb.set_text(marker.clone())?;
    let mut kb = Enigo::new(&enigo::Settings::default())?;
    for k in [Key::Shift, Key::Alt, Key::Meta] {
        let _ = kb.key(k, Direction::Release);
    }
    kb.key(Key::Control, Direction::Press)?;
    kb.key(Key::Unicode('c'), Direction::Click)?;
    kb.key(Key::Control, Direction::Release)?;
    let mut got = None;
    for _ in 0..10 {
        sleep(Duration::from_millis(30));
        match cb.get_text() {
            Ok(t) if t != marker => {
                got = Some(t);
                break;
            }
            _ => {}
        }
    }
    match previous {
        Some(p) => {
            let _ = cb.set_text(p);
        }
        None => {
            let _ = cb.clear();
        }
    }
    Ok(got.filter(|t| !t.trim().is_empty()))
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

/// Title of the focused window ("Inbox - Gmail - Google Chrome"), so the clean-up
/// can tell an email from a chat from a code editor.
#[cfg(windows)]
pub fn focused_title() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
    unsafe {
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(GetForegroundWindow(), &mut buf);
        (n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// Apps with a visible window right now (exe names, lowercased), for the Apps page.
#[cfg(windows)]
pub fn open_apps() -> Vec<String> {
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible};
    unsafe extern "system" fn each(h: HWND, lp: LPARAM) -> BOOL {
        let pids = &mut *(lp.0 as *mut Vec<u32>);
        if IsWindowVisible(h).as_bool() && GetWindowTextLengthW(h) > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid != 0 && !pids.contains(&pid) {
                pids.push(pid);
            }
        }
        BOOL(1)
    }
    let mut pids: Vec<u32> = vec![];
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut pids as *mut _ as isize));
    }
    let me = std::process::id();
    let mut names: Vec<String> = pids
        .into_iter()
        .filter(|p| *p != me)
        .filter_map(|pid| unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(h);
            ok.then(|| String::from_utf16_lossy(&buf[..len as usize]).rsplit('\\').next().unwrap_or("").to_lowercase())
        })
        .filter(|n| !n.is_empty() && !["explorer.exe", "textinputhost.exe", "applicationframehost.exe", "searchhost.exe", "shellexperiencehost.exe", "startmenuexperiencehost.exe", "systemsettings.exe", "baa.exe"].contains(&n.as_str()))
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(not(windows))]
pub fn open_apps() -> Vec<String> {
    vec![]
}

#[cfg(not(windows))]
pub fn focused_title() -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn focused_app() -> Option<String> {
    None
}
