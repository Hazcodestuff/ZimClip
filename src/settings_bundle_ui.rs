//! Windows file pickers for the settings backup. Nothing is written here.
use std::path::PathBuf;
use windows_sys::Win32::{Foundation::HWND, UI::Controls::Dialogs::*};
fn run(owner: HWND, save: bool) -> Option<PathBuf> {
    unsafe {
        let mut file = [0u16; 520];
        if save {
            for (i, c) in "ZimClip-settings.txt".encode_utf16().enumerate() {
                file[i] = c;
            }
        }
        let filter: Vec<u16> = "ZimClip settings (*.txt)\0*.txt\0\0".encode_utf16().collect();
        let mut o: OPENFILENAMEW = std::mem::zeroed();
        o.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        o.hwndOwner = owner;
        o.lpstrFilter = filter.as_ptr();
        o.lpstrFile = file.as_mut_ptr();
        o.nMaxFile = file.len() as u32;
        o.Flags = OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER
            | if save { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST };
        let ok = if save { GetSaveFileNameW(&mut o) } else { GetOpenFileNameW(&mut o) };
        if ok == 0 {
            return None;
        }
        let n = file.iter().position(|c| *c == 0).unwrap_or(0);
        Some(PathBuf::from(String::from_utf16_lossy(&file[..n])))
    }
}
pub fn pick_save(owner: HWND) -> Option<PathBuf> {
    run(owner, true)
}
pub fn pick_open(owner: HWND) -> Option<PathBuf> {
    run(owner, false)
}
