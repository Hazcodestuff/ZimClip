use std::{
    path::Path,
    process::{Command, Stdio},
};
#[derive(Clone)]
pub struct Timeline {
    pub frames: Vec<Vec<u8>>,
    pub peaks: Vec<f32>,
    pub sources: Vec<Vec<f32>>,
    /// True when the filmstrip could not be decoded; the waveform may still be valid.
    pub frames_unavailable: bool,
}
pub fn peaks(samples: &[i16], width: usize) -> Vec<f32> {
    if samples.is_empty() || width == 0 {
        return Vec::new();
    }
    (0..width)
        .map(|i| {
            let a = i * samples.len() / width;
            let b = ((i + 1) * samples.len() / width)
                .max(a + 1)
                .min(samples.len());
            samples[a..b]
                .iter()
                .map(|v| (*v as f32).abs() / 32768.0)
                .fold(0.0, f32::max)
        })
        .collect()
}
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
static ACTIVE: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
/// Stop the running analysis (editor closed or a newer one started).
pub fn cancel_all() {
    if let Some(c) = ACTIVE.lock().unwrap().take() {
        c.store(true, Ordering::SeqCst);
    }
}
#[cfg(windows)]
fn log(s: &str) {
    use std::io::Write;
    if let Some(root) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("editor-diagnostics.txt"))
        {
            writeln!(f, "{s}").ok();
        }
    }
}
/// One helper run with a time limit and cancellation. Returns (success, stdout, stderr).
#[cfg(windows)]
fn run(
    step: &str,
    helper: &Path,
    args: &[std::ffi::OsString],
    cancel: &AtomicBool,
    limit_secs: u64,
) -> Result<(bool, Vec<u8>, String), String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    let began = std::time::Instant::now();
    let mut child = Command::new(helper)
        .creation_flags(0x08000000)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let pid = child.id();
    log(&format!("Analysis step={step} start pid={pid}"));
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out_reader = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = out.read_to_end(&mut v);
        v
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let status = loop {
        if cancel.load(Ordering::SeqCst) || began.elapsed().as_secs() > limit_secs {
            let timed_out = !cancel.load(Ordering::SeqCst);
            let _ = child.kill();
            let _ = child.wait();
            let _ = out_reader.join();
            let _ = err_reader.join();
            log(&format!(
                "Analysis step={step} {} after_ms={}",
                if timed_out { "timeout" } else { "cancelled" },
                began.elapsed().as_millis()
            ));
            return Err(if timed_out {
                format!("Analysis step {step} timed out")
            } else {
                "Analysis cancelled".into()
            });
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    log(&format!(
        "Analysis step={step} pid={pid} exited code={:?} ok={} bytes={} ms={}",
        status.code(),
        status.success(),
        stdout.len(),
        began.elapsed().as_millis()
    ));
    Ok((status.success(), stdout, stderr))
}
#[cfg(windows)]
pub fn load(
    helper: &Path,
    input: &Path,
    duration: f64,
    updates: &std::sync::mpsc::Sender<Result<Timeline, String>>,
) -> Result<Timeline, String> {
    use std::ffi::OsString;
    if duration > 301.0 {
        return Err("Filmstrip and waveforms support clips up to 300 seconds. Preview and export remain available.".into());
    }
    type Cached = (std::path::PathBuf, u64, u128, Timeline);
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<Cached>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let meta = std::fs::metadata(input).map_err(|e| e.to_string())?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    if let Some((path, size, time, data)) = &*cache.lock().unwrap() {
        if path == input && *size == meta.len() && *time == modified {
            return Ok(data.clone());
        }
    }
    // One analysis at a time: a newer one (reopen) or a closed editor cancels the older one.
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(old) = ACTIVE.lock().unwrap().replace(cancel.clone()) {
        old.store(true, Ordering::SeqCst);
    }
    let seconds = duration.clamp(0.1, 301.0);
    let os = |v: &[&str]| -> Vec<OsString> { v.iter().map(OsString::from).collect() };
    let t = format!("{seconds:.3}");
    // Decoder options such as -skip_frame must come before -i, or they apply to the output.
    let with_pre = |pre: &[&str], extra: &[&str]| {
        let mut a = os(&["-v", "error", "-nostdin", "-threads", "1"]);
        a.extend(os(pre));
        a.push(OsString::from("-i"));
        a.push(input.as_os_str().to_owned());
        a.extend(os(extra));
        a
    };
    let with = |extra: &[&str]| with_pre(&[], extra);
    let pcm_args = |map: Option<String>| {
        let mut v = vec!["-t".to_string(), t.clone()];
        if let Some(m) = map {
            v.push("-map".into());
            v.push(m);
        }
        for a in ["-vn", "-ac", "1", "-ar", "8000", "-c:a", "pcm_s16le", "-f", "s16le", "pipe:1"] {
            v.push(a.into());
        }
        v
    };
    let to_pcm = |b: &[u8]| -> Vec<i16> {
        b.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    };
    let strs = |v: &Vec<String>| with(&v.iter().map(String::as_str).collect::<Vec<_>>());
    let (ok, stdout, _) = run("audio-mix", helper, &strs(&pcm_args(None)), &cancel, 30)?;
    let pcm: Vec<i16> = if ok { to_pcm(&stdout) } else { Vec::new() };
    let mut sources = Vec::new();
    if crate::media::has_separate_audio(input) {
        for track in 1..=3 {
            let (ok, stdout, _) = run(
                &format!("audio-source-{track}"),
                helper,
                &strs(&pcm_args(Some(format!("0:a:{track}")))),
                &cancel,
                30,
            )?;
            if !ok {
                return Err(format!("Source waveform {track} failed"));
            }
            sources.push(peaks(&to_pcm(&stdout), 468));
        }
    }
    let early = Timeline {
        frames: Vec::new(),
        peaks: peaks(&pcm, 468),
        sources,
        frames_unavailable: false,
    };
    let _ = updates.send(Ok(early.clone()));
    // Eight real samples at the slot times i * duration / 8, one seek each. A keyframe-only
    // fps pass drops the last slot (the final keyframe has no end time) and covers sparse
    // keyframes badly, so each sample is decoded at its own time instead. A slot that cannot be
    // decoded stays empty (a real gap), never a copy of another frame.
    let scale = "scale=112:64:force_original_aspect_ratio=decrease,pad=112:64:(ow-iw)/2:(oh-ih)/2";
    const FRAME_BYTES: usize = 112 * 64 * 4;
    let budget = std::time::Instant::now();
    let mut frames: Vec<Vec<u8>> = Vec::new();
    for slot in 0..8 {
        let left = 45u64.saturating_sub(budget.elapsed().as_secs());
        if left == 0 {
            frames.push(Vec::new());
            continue;
        }
        let at = format!("{:.3}", seconds * slot as f64 / 8.0);
        let got = run(
            &format!("filmstrip-{slot}"),
            helper,
            &with_pre(&["-ss", &at], &["-an", "-vf", scale, "-frames:v", "1", "-pix_fmt", "bgra", "-c:v", "rawvideo", "-f", "rawvideo", "pipe:1"]),
            &cancel,
            left.min(15),
        );
        match got {
            Ok((_, stdout, _)) if stdout.len() >= FRAME_BYTES => frames.push(stdout[..FRAME_BYTES].to_vec()),
            Ok(_) => frames.push(Vec::new()),
            Err(e) if e.contains("timed out") => frames.push(Vec::new()),
            Err(e) => return Err(e),
        }
    }
    let sampled = frames.iter().filter(|f| !f.is_empty()).count();
    log(&format!("Analysis filmstrip sampled={sampled}/8 ms={}", budget.elapsed().as_millis()));
    let unavailable = sampled == 0;
    if unavailable {
        frames.clear();
    }
    let result = Timeline {
        frames,
        peaks: early.peaks,
        sources: early.sources,
        frames_unavailable: unavailable,
    };
    *cache.lock().unwrap() = Some((input.to_path_buf(), meta.len(), modified, result.clone()));
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence() {
        assert_eq!(peaks(&[0; 8], 4), vec![0.0; 4]);
    }
    #[test]
    fn signed_peak() {
        assert_eq!(peaks(&[-32768, 0, 16384, -8192], 2), vec![1.0, 0.5]);
    }
    #[test]
    fn small() {
        assert_eq!(peaks(&[16384], 3), vec![0.5; 3]);
        assert!(peaks(&[], 3).is_empty());
    }
}
