#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioMix {
    pub system: u32,
    pub mic: u32,
    pub comms: u32,
}
impl Default for AudioMix {
    fn default() -> Self {
        Self {
            system: 100,
            mic: 100,
            comms: 100,
        }
    }
}
/// Mix an editor starts from: a layout switch reopens the same clip and keeps its gains;
/// any other clip starts from the default.
pub fn start_mix(carried: Option<(&std::path::Path, AudioMix)>, input: &std::path::Path) -> AudioMix {
    match carried {
        Some((path, mix)) if path == input => mix,
        _ => AudioMix::default(),
    }
}
impl AudioMix {
    pub fn graph(self) -> String {
        format!("[0:a:1]volume={:.4}[system];[0:a:2]volume={:.4}[mic];[0:a:3]volume={:.4}[comms];[system][mic][comms]amix=inputs=3:normalize=0[mix]",self.system.min(200) as f64/100.0,self.mic.min(200) as f64/100.0,self.comms.min(200) as f64/100.0)
    }
}
#[cfg(windows)]
pub fn preview(
    helper: &std::path::Path,
    input: &std::path::Path,
    mix: AudioMix,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<std::path::PathBuf, String> {
    use std::os::windows::process::CommandExt;
    let folder = std::env::temp_dir().join(format!("zimclip-audition-{}", std::process::id()));
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join(format!(
        "{}.mp4",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut child = std::process::Command::new(helper)
        .creation_flags(0x08000000)
        .args(["-v", "error", "-nostdin", "-n", "-i"])
        .arg(input)
        .args([
            "-filter_complex",
            &mix.graph(),
            "-map",
            "0:v:0",
            "-map",
            "[mix]",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-movflags",
            "+faststart",
        ])
        .arg(&path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stderr = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let began = std::time::Instant::now();
    let result = loop {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) || began.elapsed().as_secs() > 90 {
            let _ = child.kill();
            let _ = child.wait();
            break Err("Audition cancelled or timed out. Previous mix kept.".to_string());
        }
        match child.try_wait() {
            Ok(Some(s)) => {
                break if s.success() {
                    Ok(())
                } else {
                    Err("Audition encoder failed".into())
                }
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(30)),
            Err(e) => break Err(e.to_string()),
        }
    };
    let error = reader.join().unwrap_or_default();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&path);
        return Err(format!("{e}: {}", error));
    }
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_switch_keeps_mix_but_other_clip_resets() {
        use std::path::Path;
        let m = AudioMix { system: 100, mic: 0, comms: 40 };
        let a = Path::new("a.mp4");
        assert_eq!(start_mix(Some((a, m)), a), m);
        assert_eq!(start_mix(Some((a, m)), Path::new("b.mp4")), AudioMix::default());
        assert_eq!(start_mix(None, a), AudioMix::default());
    }
    #[test]
    fn independently_scaled() {
        let s = AudioMix {
            system: 0,
            mic: 50,
            comms: 200,
        }
        .graph();
        assert!(s.contains("volume=0.0000"));
        assert!(s.contains("volume=0.5000"));
        assert!(s.contains("volume=2.0000"));
        assert!(s.contains("normalize=0"));
    }
}
