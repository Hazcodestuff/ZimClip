//! Rounded popup menus. The system menu window keeps a square frame on current Windows, so the
//! app's menus are drawn by `popup_menu` instead; this keeps the one call the menus go through.
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::HMENU};

/// Shows the menu at (x, y) and returns the chosen command id, or 0. Same arguments and result
/// as `TrackPopupMenu` with `TPM_RETURNCMD`.
pub unsafe fn track(menu: HMENU, flags: u32, x: i32, y: i32, owner: HWND) -> i32 {
    crate::popup_menu::track(menu, flags, x, y, owner)
}
