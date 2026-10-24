//! Mutes other apps' sound (music, videos, calls) while you dictate, then puts it
//! back. Works per app through the Windows audio mixer, so Baa's own chimes still
//! play and anything you had muted yourself stays muted.

use parking_lot::Mutex;

/// Processes we muted, so only those get unmuted again.
static MUTED: Mutex<Vec<u32>> = Mutex::new(Vec::new());

#[cfg(windows)]
mod imp {
    use windows::core::Interface;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
        MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };

    /// Our process and everything it started (the WebView2 windows play our chimes).
    fn ours() -> Vec<u32> {
        let me = std::process::id();
        let mut family = vec![me];
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return family };
            let mut all = vec![];
            let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            if Process32FirstW(snap, &mut e).is_ok() {
                loop {
                    all.push((e.th32ProcessID, e.th32ParentProcessID));
                    if Process32NextW(snap, &mut e).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
            // children, grandchildren...
            let mut grew = true;
            while grew {
                grew = false;
                for (pid, parent) in &all {
                    if family.contains(parent) && !family.contains(pid) {
                        family.push(*pid);
                        grew = true;
                    }
                }
            }
        }
        family
    }

    /// Calls `f(pid, volume)` for every app playing on the default speakers.
    fn each_session(mut f: impl FnMut(u32, &ISimpleAudioVolume)) -> windows::core::Result<()> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let speakers = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let mgr: IAudioSessionManager2 = speakers.Activate(CLSCTX_ALL, None)?;
            let list = mgr.GetSessionEnumerator()?;
            for i in 0..list.GetCount()? {
                let Ok(ctl) = list.GetSession(i) else { continue };
                let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                if ctl2.IsSystemSoundsSession().0 == 0 {
                    continue; // S_OK means it IS the system sounds session
                }
                let Ok(pid) = ctl2.GetProcessId() else { continue };
                let Ok(vol) = ctl.cast::<ISimpleAudioVolume>() else { continue };
                f(pid, &vol);
            }
        }
        Ok(())
    }

    pub fn mute(muted: &mut Vec<u32>) {
        let mine = ours();
        let _ = each_session(|pid, vol| unsafe {
            if mine.contains(&pid) || muted.contains(&pid) {
                return;
            }
            if vol.GetMute().map(|m| m.as_bool()).unwrap_or(true) {
                return; // already muted by the user: leave it, and leave it muted after
            }
            if vol.SetMute(true, std::ptr::null()).is_ok() {
                muted.push(pid);
            }
        });
    }

    pub fn restore(muted: &mut Vec<u32>) {
        let _ = each_session(|pid, vol| unsafe {
            if muted.contains(&pid) {
                let _ = vol.SetMute(false, std::ptr::null());
            }
        });
        muted.clear();
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn mute(_m: &mut Vec<u32>) {}
    pub fn restore(_m: &mut Vec<u32>) {}
}

pub fn mute_others() {
    imp::mute(&mut MUTED.lock());
}

pub fn restore() {
    imp::restore(&mut MUTED.lock());
}
