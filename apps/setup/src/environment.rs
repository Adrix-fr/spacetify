//! Tells Windows the user environment changed, so terminals opened from the
//! Start menu or the taskbar see the new `Path` without logging out.

use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};

/// Best effort: if a window does not answer, it only misses the news.
#[allow(unsafe_code)]
pub fn announce_change() {
    let area: Vec<u16> = "Environment\0".encode_utf16().collect();
    let mut result = 0usize;
    // SAFETY: the message only reads the null-terminated string, which
    // outlives the call (it waits at most 5 s per window).
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            area.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result,
        );
    }
}
