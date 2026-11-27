use std::{path::PathBuf, sync::mpsc::Sender};
use windows::{
    core::PCWSTR,
    Win32::{Foundation::SIZE, System::Com::*, UI::Shell::*},
};
pub fn request(paths: Vec<PathBuf>, sender: Sender<(PathBuf, usize)>) {
    std::thread::spawn(move || unsafe {
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
            return;
        }
        for path in paths {
            let wide: Vec<u16> = path
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let item: Result<IShellItemImageFactory, _> =
                SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None);
            if let Ok(item) = item {
                if let Ok(bitmap) = item.GetImage(
                    SIZE { cx: 320, cy: 180 },
                    SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK,
                ) {
                    if let Err(err) = sender.send((path, bitmap.0 as usize)) {
                        windows_sys::Win32::Graphics::Gdi::DeleteObject(err.0 .1 as _);
                    }
                }
            }
        }
        CoUninitialize();
    });
}
