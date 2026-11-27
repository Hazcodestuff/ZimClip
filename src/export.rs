use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Fast,
    Exact,
    Share720,
    Small480,
    Target10,
    Target20,
    TargetCustom(u32),
}
pub fn video_args(mode: Mode) -> Vec<String> {
    let mut a = vec![
        "-c:v".into(),
        "libopenh264".into(),
        "-threads".into(),
        "2".into(),
        "-rc_mode".into(),
        "bitrate".into(),
        "-allow_skip_frames".into(),
        "0".into(),
    ];
    let (size, rate) = match mode {
        Mode::Share720 => (Some((1280, 720)), "1800k"),
        Mode::Small480 => (Some((854, 480)), "850k"),
        _ => (None, "6000k"),
    };
    if let Some((w, h)) = size {
        a.extend(["-vf".into(), format!("scale=w='min({w},iw)':h='min({h},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2")]);
    }
    a.extend([
        "-b:v".into(),
        rate.into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "128k".into(),
    ]);
    a
}
pub fn target_bytes(mode: Mode) -> Option<u64> {
    match mode {
        Mode::Target10 => Some(10_000_000),
        Mode::Target20 => Some(20_000_000),
        Mode::TargetCustom(m) => Some(m.clamp(1, 500) as u64 * 1_000_000),
        _ => None,
    }
}
#[cfg(windows)]
pub fn export(
    engine: &Path,
    input: &Path,
    start: f64,
    end: f64,
    mode: Mode,
    mix: Option<crate::mixer::AudioMix>,
    cancel: Arc<AtomicBool>,
    progress: Sender<u32>,
) -> Result<PathBuf, String> {
    export_attempt(engine, input, start, end, mode, mix, cancel, progress, 0)
}
#[cfg(windows)]
fn export_attempt(
    engine: &Path,
    input: &Path,
    start: f64,
    end: f64,
    mode: Mode,
    mix: Option<crate::mixer::AudioMix>,
    cancel: Arc<AtomicBool>,
    progress: Sender<u32>,
    attempt: u8,
) -> Result<PathBuf, String> {
    use std::os::windows::process::CommandExt;
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
        return Err("Choose a nonempty range".into());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let prefix = match mode {
        Mode::Fast => "trim",
        Mode::Exact => "precise",
        Mode::Share720 => "share720",
        Mode::Small480 => "small480",
        Mode::Target10 => "under10mb",
        Mode::Target20 => "under20mb",
        Mode::TargetCustom(_) => "sized",
    };
    let out = input
        .parent()
        .unwrap()
        .join(format!("{prefix}-{stamp}.mp4"));
    let partial = out.with_extension("partial.mp4");
    let helper = if mode == Mode::Fast && mix.is_none() {
        engine.to_path_buf()
    } else {
        engine.parent().unwrap().join("media-export.exe")
    };
    let mut cmd = Command::new(helper);
    cmd.creation_flags(0x08000000).args([
        "-v",
        "error",
        "-nostdin",
        "-n",
        "-progress",
        "pipe:1",
        "-nostats",
    ]);
    if mode == Mode::Fast {
        cmd.args(["-ss", &format!("{start:.6}")]);
    }
    cmd.arg("-i").arg(input);
    if mode != Mode::Fast {
        cmd.args(["-ss", &format!("{start:.6}")]);
    }
    cmd.args(["-t", &format!("{:.6}", end - start), "-map", "0:v:0"]);
    if let Some(m) = mix {
        cmd.args([
            "-filter_complex",
            &m.graph(),
            "-map",
            "[mix]",
            "-map",
            "0:a:1",
            "-map",
            "0:a:2",
            "-map",
            "0:a:3",
        ]);
    } else {
        cmd.args(["-map", "0:a?"]);
    }
    if mode == Mode::Fast {
        cmd.args(["-c", "copy", "-avoid_negative_ts", "make_zero"]);
    } else {
        let mut args = video_args(mode);
        if let Some(cap) = target_bytes(mode) {
            let cap = cap as f64;
            let bitrate = ((cap * (0.92 * 0.72f64.powi(attempt as i32)) * 8.0 / (end - start))
                - if crate::media::has_separate_audio(input) {
                    512000.0
                } else {
                    128000.0
                })
            .clamp(64000.0, 40000000.0) as u64;
            let i = args.iter().position(|a| a == "-b:v").unwrap();
            args[i + 1] = bitrate.to_string();
            args.extend([
                "-qmin".into(),
                (20 + attempt as u32 * 12).to_string(),
                "-qmax".into(),
                "51".into(),
            ]);
        }
        cmd.args(args);
    }
    if mix.is_some() {
        cmd.args([
            "-c:a:0",
            "aac",
            "-b:a:0",
            "128k",
            "-metadata:s:a:0",
            "handler_name=ZimClip Mix",
            "-disposition:a:0",
            "default",
            "-disposition:a:1",
            "0",
            "-disposition:a:2",
            "0",
            "-disposition:a:3",
            "0",
        ]);
    }
    cmd.args(["-movflags", "+faststart"])
        .arg(&partial)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        stderr.read_to_string(&mut s).ok();
        s
    });
    let duration = end - start;
    let retry_progress = progress.clone();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(t) = line
                .strip_prefix("out_time_us=")
                .and_then(|v| v.parse::<f64>().ok())
            {
                let _ = progress.send((t / 1e6 / duration * 100.0).clamp(0.0, 99.0) as u32);
            }
        }
    });
    let result = loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break Err("Export cancelled. Original unchanged.".to_string());
        }
        match child.try_wait() {
            Ok(Some(s)) => {
                break if s.success() {
                    Ok(())
                } else {
                    Err("Encoder failed".into())
                }
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => break Err(e.to_string()),
        }
    };
    let _ = reader.join();
    let error = err_thread.join().unwrap_or_default();
    if let Err(e) = result {
        std::fs::remove_file(&partial).ok();
        return Err(if error.is_empty() {
            e
        } else {
            format!("{e}: {error}")
        });
    }
    let target = target_bytes(mode);
    if let Some(cap) = target {
        if std::fs::metadata(&partial)
            .map_err(|e| e.to_string())?
            .len()
            > cap
        {
            std::fs::remove_file(&partial).ok();
            if attempt < 2 && !cancel.load(Ordering::SeqCst) {
                return export_attempt(
                    engine,
                    input,
                    start,
                    end,
                    mode,
                    mix,
                    cancel,
                    retry_progress,
                    attempt + 1,
                );
            }
            return Err("Could not fit the size limit at this range. Try a shorter range or the 480p preset. Original unchanged.".into());
        }
    }
    if out.exists() {
        std::fs::remove_file(&partial).ok();
        return Err("Output already exists, not overwritten".into());
    }
    std::fs::rename(&partial, &out).map_err(|e| e.to_string())?;
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compression_presets() {
        let a = video_args(Mode::Share720);
        assert!(a.iter().any(|v| v.contains("1280")));
        assert!(a.contains(&"1800k".to_string()));
        let b = video_args(Mode::Exact);
        assert!(!b.contains(&"-vf".to_string()));
    }
    #[test]
    fn custom_target_bounds() {
        assert_eq!(target_bytes(Mode::TargetCustom(1)), Some(1_000_000));
        assert_eq!(target_bytes(Mode::TargetCustom(999)), Some(500_000_000));
    }
    #[test]
    fn no_frame_skip() {
        assert!(video_args(Mode::Exact)
            .windows(2)
            .any(|v| v == ["-allow_skip_frames", "0"]));
    }
}
