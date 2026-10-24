//! Modifier-only shortcuts like Ctrl+Win, which normal global hotkeys can't do.
//! A low-level keyboard hook watches the modifier keys: when exactly the chosen
//! set is held we report "pressed", when one is let go we report "released".
//! Any other key pressed during the chord (Ctrl+Win+D...) aborts it, so normal
//! Windows shortcuts keep working.
//!
//! The same thread also watches the mouse side buttons, so Mouse 4 / Mouse 5 can
//! be the talk button (their usual Back/Forward is swallowed while assigned).

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::OnceLock;

pub const CTRL: u8 = 1;
pub const ALT: u8 = 2;
pub const SHIFT: u8 = 4;
pub const WIN: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChordEvent {
    Pressed,
    Released,
    /// another key joined the chord: it was a regular shortcut, not dictation
    Aborted,
}

/// Parses "Ctrl+Win" style strings. Returns Some(mask) only when every part is a modifier.
pub fn parse_modifier_only(s: &str) -> Option<u8> {
    let mut mask = 0u8;
    for part in s.split('+') {
        mask |= match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => CTRL,
            "alt" => ALT,
            "shift" => SHIFT,
            "win" | "super" | "meta" | "cmd" => WIN,
            _ => return None,
        };
    }
    (mask.count_ones() >= 2).then_some(mask)
}

static WANT: AtomicU8 = AtomicU8::new(0); // 0 = hook idle
static HELD: AtomicU8 = AtomicU8::new(0);
static STATE: AtomicU8 = AtomicU8::new(0); // 0 idle, 1 chord active, 2 aborted (wait for all up)
/// 0 none, 1 = Mouse 4 (XBUTTON1), 2 = Mouse 5 (XBUTTON2)
static MOUSE: AtomicU8 = AtomicU8::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);
static HANDLER: OnceLock<Box<dyn Fn(ChordEvent) + Send + Sync>> = OnceLock::new();

/// Sets the chord to watch (0 disables). The hook thread starts on first use.
pub fn set_chord(mask: u8, handler: impl Fn(ChordEvent) + Send + Sync + 'static) {
    let _ = HANDLER.set(Box::new(handler));
    WANT.store(mask, Ordering::SeqCst);
    STATE.store(0, Ordering::SeqCst);
    ensure_thread();
}

/// "mouse4" | "mouse5" | "" (off). Uses the same handler as the chord.
pub fn set_mouse(button: &str) {
    MOUSE.store(
        match button {
            "mouse4" => 1,
            "mouse5" => 2,
            _ => 0,
        },
        Ordering::SeqCst,
    );
    ensure_thread();
}

fn ensure_thread() {
    #[cfg(windows)]
    if (WANT.load(Ordering::SeqCst) != 0 || MOUSE.load(Ordering::SeqCst) != 0) && THREAD.load(Ordering::SeqCst) == 0 {
        win::start();
    }
}

fn emit(e: ChordEvent) {
    if let Some(h) = HANDLER.get() {
        h(e);
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL,
        VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, MSLLHOOKSTRUCT,
        WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
    };

    fn modifier_bit(vk: u16) -> u8 {
        let vk = VIRTUAL_KEY(vk);
        if [VK_CONTROL, VK_LCONTROL, VK_RCONTROL].contains(&vk) {
            CTRL
        } else if [VK_MENU, VK_LMENU, VK_RMENU].contains(&vk) {
            ALT
        } else if [VK_SHIFT, VK_LSHIFT, VK_RSHIFT].contains(&vk) {
            SHIFT
        } else if [VK_LWIN, VK_RWIN].contains(&vk) {
            WIN
        } else {
            0
        }
    }

    /// Tapping an unassigned key before Win comes up stops the Start menu opening.
    fn mask_start_menu() {
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0xE8), dwFlags: flags, ..Default::default() } },
        };
        unsafe {
            SendInput(&[key(Default::default()), key(KEYEVENTF_KEYUP)], std::mem::size_of::<INPUT>() as i32);
        }
    }

    unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let want = WANT.load(Ordering::Relaxed);
        if code >= 0 && want != 0 {
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            if (kb.flags.0 & LLKHF_INJECTED.0) == 0 {
                let msg = wparam.0 as u32;
                let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
                let bit = modifier_bit(kb.vkCode as u16);
                let state = STATE.load(Ordering::Relaxed);
                if bit != 0 {
                    let held = HELD.load(Ordering::Relaxed);
                    if down {
                        let now = held | bit;
                        HELD.store(now, Ordering::Relaxed);
                        if state == 0 && now == want {
                            STATE.store(1, Ordering::Relaxed);
                            emit(ChordEvent::Pressed);
                        } else if state == 1 && now != want {
                            STATE.store(2, Ordering::Relaxed);
                            emit(ChordEvent::Aborted);
                        }
                    } else if up {
                        let now = held & !bit;
                        HELD.store(now, Ordering::Relaxed);
                        if state == 1 {
                            STATE.store(if now == 0 { 0 } else { 2 }, Ordering::Relaxed);
                            if want & WIN != 0 {
                                mask_start_menu();
                            }
                            emit(ChordEvent::Released);
                        } else if now == 0 {
                            STATE.store(0, Ordering::Relaxed);
                        }
                    }
                } else if down && state == 1 {
                    STATE.store(2, Ordering::Relaxed);
                    emit(ChordEvent::Aborted);
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let want = MOUSE.load(Ordering::Relaxed) as u32;
        if code >= 0 && want != 0 {
            let msg = wparam.0 as u32;
            if msg == WM_XBUTTONDOWN || msg == WM_XBUTTONUP {
                let m = &*(lparam.0 as *const MSLLHOOKSTRUCT);
                // high word of mouseData: 1 = XBUTTON1 (Mouse 4), 2 = XBUTTON2 (Mouse 5)
                if (m.mouseData >> 16) & 0xffff == want {
                    emit(if msg == WM_XBUTTONDOWN { ChordEvent::Pressed } else { ChordEvent::Released });
                    return LRESULT(1); // ours: don't also go Back/Forward
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    pub fn start() {
        std::thread::Builder::new()
            .name("baa-keyhook".into())
            .spawn(|| unsafe {
                THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
                let _h = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), None, 0);
                let _m = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0);
                let mut msg = MSG::default();
                // low-level hooks need a message loop on the installing thread
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
            })
            .expect("key hook thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        assert_eq!(parse_modifier_only("Ctrl+Win"), Some(CTRL | WIN));
        assert_eq!(parse_modifier_only("Ctrl+Space"), None);
        assert_eq!(parse_modifier_only("Ctrl"), None);
    }
}
