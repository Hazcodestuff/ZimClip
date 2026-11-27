use std::path::{Path, PathBuf};
pub fn load(root: &Path) -> PathBuf {
    std::fs::read_to_string(root.join("video-folder.txt"))
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| root.join("Clips"))
}
pub fn save(root: &Path, folder: &Path) -> Result<(), String> {
    if !folder.is_absolute() {
        return Err("Choose an absolute folder path".into());
    }
    std::fs::create_dir_all(folder).map_err(|e| format!("Cannot create video folder: {e}"))?;
    let test = folder.join(format!(".zimclip-write-check-{}", std::process::id()));
    let f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&test)
        .map_err(|e| format!("Video folder is not writable: {e}"))?;
    drop(f);
    std::fs::remove_file(test).map_err(|e| e.to_string())?;
    atomic_text(&root.join("video-folder.txt"), &folder.to_string_lossy())
}
#[cfg(windows)]
pub unsafe fn choose(
    parent: windows_sys::Win32::Foundation::HWND,
    current: &Path,
) -> Result<Option<PathBuf>, String> {
    let owner = parent as usize;
    let initial = current.to_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let answer = (|| unsafe {
            use windows::{
                core::PCWSTR,
                Win32::{Foundation::HWND, System::Com::*, UI::Shell::*},
            };
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| e.to_string())?;
            let result = (|| {
                let d: IFileOpenDialog =
                    CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                        .map_err(|e| e.to_string())?;
                d.SetOptions(
                    FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR,
                )
                .map_err(|e| e.to_string())?;
                let title: Vec<u16> =
                    "Choose video save folder (existing clips stay where they are)"
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                d.SetTitle(PCWSTR(title.as_ptr()))
                    .map_err(|e| e.to_string())?;
                let path: Vec<u16> = initial
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                if let Ok(item) =
                    SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(path.as_ptr()), None)
                {
                    let _ = d.SetFolder(&item);
                }
                if let Err(e) = d.Show(Some(HWND(owner as _))) {
                    if e.code().0 == 0x800704c7u32 as i32 {
                        return Ok(None);
                    }
                    return Err(e.to_string());
                }
                let item = d.GetResult().map_err(|e| e.to_string())?;
                let name = item
                    .GetDisplayName(SIGDN_FILESYSPATH)
                    .map_err(|e| e.to_string())?;
                let path = PathBuf::from(name.to_string().map_err(|e| e.to_string())?);
                CoTaskMemFree(Some(name.0 as _));
                Ok(Some(path))
            })();
            CoUninitialize();
            result
        })();
        let _ = tx.send(answer);
    });
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    EnableWindow(parent, 0);
    let mut msg = MSG::default();
    let answer = loop {
        match rx.try_recv() {
            Ok(a) => break a,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                break Err("Folder picker stopped".into())
            }
            Err(_) => {}
        }
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                PostQuitMessage(msg.wParam as i32);
                EnableWindow(parent, 1);
                return Ok(None);
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let _ = worker.join();
    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    answer
}

pub fn atomic_text(path: &Path, text: &str) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, text).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let a: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_persistence_preserves_videos() {
        let d = std::env::temp_dir().join(format!("zim-folder-{}", std::process::id()));
        let old = d.join("Clips");
        let new = d.join("New");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("keep.mp4"), b"keep-video").unwrap();
        assert_eq!(load(&d), old);
        save(&d, &new).unwrap();
        assert_eq!(load(&d), new);
        assert_eq!(std::fs::read(old.join("keep.mp4")).unwrap(), b"keep-video");
        assert!(save(&d, Path::new("relative")).is_err());
        assert_eq!(load(&d), new);
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn switching_folders_keeps_independent_tags_colors_and_video_bytes() {
        let d = std::env::temp_dir().join(format!("zim-folder-metadata-{}", std::process::id()));
        let old = d.join("Clips");
        let new = d.join("New");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        let a = old.join("same.mp4");
        let b = new.join("same.mp4");
        std::fs::write(&a, b"old-original").unwrap();
        std::fs::write(&b, b"new-original").unwrap();
        crate::catalog::set_tags(&a, "Ballers").unwrap();
        crate::catalog::save_color(&old, "Ballers", 0x123456).unwrap();
        crate::catalog::set_tags(&b, "Private").unwrap();
        crate::catalog::save_color(&new, "Private", 0xabcdef).unwrap();
        for target in [&new, &old, &new, &old] {
            save(&d, target).unwrap();
            let active = load(&d);
            assert_eq!(&active, target);
            if active == old {
                assert_eq!(crate::catalog::tags(&active)["same.mp4"], "Ballers");
                assert_eq!(
                    crate::catalog::reusable(&active).unwrap()["Ballers"],
                    0x123456
                );
            } else {
                assert_eq!(crate::catalog::tags(&active)["same.mp4"], "Private");
                assert_eq!(
                    crate::catalog::reusable(&active).unwrap()["Private"],
                    0xabcdef
                );
            }
        }
        assert_eq!(std::fs::read(a).unwrap(), b"old-original");
        assert_eq!(std::fs::read(b).unwrap(), b"new-original");
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn failed_folder_choice_preserves_previous_preference() {
        let d = std::env::temp_dir().join(format!("zim-folder-failed-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let valid = d.join("Good");
        save(&d, &valid).unwrap();
        let blocked = d.join("NotAFolder");
        std::fs::write(&blocked, b"keep").unwrap();
        assert!(save(&d, &blocked).is_err());
        assert_eq!(load(&d), valid);
        assert_eq!(std::fs::read(blocked).unwrap(), b"keep");
        std::fs::remove_dir_all(d).unwrap();
    }
}
