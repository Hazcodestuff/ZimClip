use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        mpsc::{self, Sender, SyncSender},
        Arc, Mutex,
    },
};
#[derive(Default)]
pub struct Tap {
    sender: Option<SyncSender<Vec<u8>>>,
    fault: Option<String>,
}
pub type Shared = Arc<Mutex<Tap>>;
pub fn new() -> Shared {
    Arc::new(Mutex::new(Tap::default()))
}
pub fn feed(tap: &Shared, init: &[u8], fragment: &[u8]) {
    let mut t = tap.lock().unwrap();
    if let Some(tx) = &t.sender {
        let mut v = Vec::with_capacity(init.len() + fragment.len());
        v.extend_from_slice(init);
        v.extend_from_slice(fragment);
        if tx.try_send(v).is_err() {
            t.fault = Some(
                "Recording could not keep up with disk writes. Saved portion may be shorter."
                    .into(),
            );
            t.sender.take();
        }
    }
}
pub fn stop(tap: &Shared) {
    tap.lock().unwrap().sender.take();
}
pub fn start(
    tap: &Shared,
    dir: &Path,
    helper: PathBuf,
    done: Sender<Result<PathBuf, String>>,
) -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let raw = dir.join(format!("recording-{stamp}.recoverable.mp4"));
    let out = dir.join(format!("recording-{stamp}.mp4"));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&raw)
        .map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(8);
    {
        let mut t = tap.lock().unwrap();
        t.sender = Some(tx);
        t.fault = None;
    }
    let shared = tap.clone();
    std::thread::spawn(move || {
        let mut first = true;
        let mut wrote = false;
        let mut error = None;
        while let Ok(bytes) = rx.recv() {
            let data = if first {
                first = false;
                &bytes[..]
            } else {
                let mut n = 0usize;
                while n + 8 <= bytes.len() {
                    let len = u32::from_be_bytes(bytes[n..n + 4].try_into().unwrap()) as usize;
                    if &bytes[n + 4..n + 8] == b"moof" {
                        break;
                    }
                    if len < 8 || n + len > bytes.len() {
                        break;
                    }
                    n += len;
                }
                &bytes[n..]
            };
            let mut free = 0u64;
            let wide: Vec<u16> = raw
                .parent()
                .unwrap_or(Path::new("."))
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            let available = unsafe {
                windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                    wide.as_ptr(),
                    &mut free,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if available != 0 && free < 256 * 1024 * 1024 + data.len() as u64 {
                error = Some("Recording stopped: less than 256 MiB free disk space".into());
                stop(&shared);
                break;
            }
            if let Err(e) = f.write_all(data).and_then(|_| f.sync_data()) {
                error = Some(format!("Disk write failed: {e}"));
                stop(&shared);
                break;
            }
            wrote = true;
        }
        if let Err(e) = f.sync_all() {
            error = Some(format!("Disk flush failed: {e}"));
        }
        drop(f);
        let fault = shared.lock().unwrap().fault.take();
        let result = if !wrote {
            if error.is_none() {
                let _ = fs::remove_file(&raw);
            }
            Err(error.unwrap_or_else(|| "Recording stopped before any video arrived.".into()))
        } else {
            finalize(&helper, &raw, &out).and_then(|p| {
                if let Some(e) = error.or(fault) {
                    Err(format!("{e} Playable portion saved at {}", p.display()))
                } else {
                    Ok(p)
                }
            })
        };
        let _ = done.send(result);
    });
    Ok(())
}
pub fn finalize(helper: &Path, raw: &Path, out: &Path) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let partial = out.with_extension(format!("{stamp}.partial.mp4"));
    let result = Command::new(helper)
        .creation_flags(0x08000000)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-n", "-i"])
        .arg(raw)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-c",
            "copy",
            "-avoid_negative_ts",
            "make_zero",
            "-movflags",
            "+faststart",
        ])
        .arg(&partial)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("{e}. Raw recording kept at {}", raw.display()))?;
    if !result.status.success() {
        return Err(format!(
            "Could not finish recording. Raw kept at {}. {}",
            raw.display(),
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let length = crate::media::duration(&partial).map_err(|e| {
        format!(
            "Recording validation failed: {e}. Raw kept at {}",
            raw.display()
        )
    })?;
    if length <= 0.0 {
        return Err(format!(
            "Recording has no duration. Raw kept at {}",
            raw.display()
        ));
    }
    if out.exists() {
        return Err(format!("Destination exists. Raw kept at {}", raw.display()));
    }
    fs::rename(&partial, out).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(raw);
    Ok(out.to_owned())
}
pub fn recover(dir: PathBuf, helper: PathBuf, done: Sender<Result<PathBuf, String>>) {
    std::thread::spawn(move || {
        if let Ok(files) = fs::read_dir(dir) {
            for file in files.flatten() {
                let p = file.path();
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    if let Some(stem) = n.strip_suffix(".recoverable.mp4") {
                        let out = p.with_file_name(format!("{stem}-recovered.mp4"));
                        let _ = done.send(finalize(&helper, &p, &out));
                        return;
                    }
                }
            }
        }
        let _ = done.send(Err("No recoverable recording was found.".into()));
    });
}
pub fn pending(dir: &Path) -> bool {
    fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
        e.file_name()
            .to_string_lossy()
            .ends_with(".recoverable.mp4")
    })
}
