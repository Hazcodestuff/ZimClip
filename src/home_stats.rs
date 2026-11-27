//! Real-state text for Home stat cards. Pure formatting only: callers pass values the app
//! already holds (capture settings, library scan). Nothing here invents a number.

pub fn replay_length(seconds: usize) -> String {
    if seconds >= 60 && seconds % 60 == 0 {
        format!("{} min", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}
pub fn capture(width: u32, fps: u32, compat_path: bool) -> (String, String) {
    // Only the output width is known (not height), so no "720p" style label is invented.
    let line = format!("{fps} fps");
    let path = if compat_path { "CPU" } else { "Native" };
    (line, format!("{width}w, {path}"))
}
pub fn size(bytes: u64) -> String {
    // Decimal units, the same as the Editor header and the clip cards.
    let mb = bytes as f64 / 1_000_000.0;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0)
    } else {
        format!("{mb:.1} MB")
    }
}
pub fn library(count: usize, bytes: u64) -> String {
    match count {
        0 => "No clips yet".to_string(),
        1 => format!("1 clip, {}", size(bytes)),
        n => format!("{n} clips, {}", size(bytes)),
    }
}
/// What the Home panel shows. Filled by the app from real state; empty until then.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Info {
    pub replay: String,
    pub capture: String,
    pub capture_sub: String,
    pub library: String,
    pub library_sub: String,
    /// Newest first: (file name without extension, "age, size").
    pub recent: Vec<(String, String)>,
    pub have_capture: bool,
    pub have_library: bool,
}
static INFO: std::sync::Mutex<Option<Info>> = std::sync::Mutex::new(None);
static HOME_PATHS: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());
pub fn set_home_paths(p: Vec<std::path::PathBuf>) {
    *HOME_PATHS.lock().unwrap() = p;
}
pub fn home_path(i: usize) -> Option<std::path::PathBuf> {
    HOME_PATHS.lock().unwrap().get(i).cloned()
}
pub fn info() -> Info {
    INFO.lock().unwrap().clone().unwrap_or_default()
}
/// Returns true when something changed (caller repaints).
pub fn set_capture(seconds: usize, width: u32, fps: u32, compat: bool) -> bool {
    let mut g = INFO.lock().unwrap();
    let mut i = g.clone().unwrap_or_default();
    let (c, sub) = capture(width, fps, compat);
    i.replay = replay_length(seconds);
    i.capture = c;
    i.capture_sub = sub;
    i.have_capture = true;
    let changed = g.as_ref() != Some(&i);
    *g = Some(i);
    changed
}
pub fn set_library(count: usize, bytes: u64, recent: Vec<(String, String)>) -> bool {
    let mut g = INFO.lock().unwrap();
    let mut i = g.clone().unwrap_or_default();
    let text = library(count, bytes);
    let (a, b) = match text.split_once(", ") {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (text, String::new()),
    };
    i.library = a;
    i.library_sub = b;
    i.recent = recent.into_iter().take(5).collect();
    i.have_library = true;
    let changed = g.as_ref() != Some(&i);
    *g = Some(i);
    changed
}
/// "just now", "12 min ago", "3 h ago", "2 d ago".
pub fn age(secs: u64) -> String {
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86399 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86400),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats() {
        assert_eq!(replay_length(30), "30s");
        assert_eq!(replay_length(120), "2 min");
        assert_eq!(capture(1280, 30, true), ("30 fps".to_string(), "1280w, CPU".to_string()));
        assert_eq!(library(0, 0), "No clips yet");
        assert_eq!(library(1, 1048576), "1 clip, 1.0 MB");
        assert_eq!(library(5, 191_889_408), "5 clips, 191.9 MB");
        assert_eq!(size(3_000_000_000), "3.0 GB");
        assert_eq!(age(30), "just now");
        assert_eq!(age(120), "2 min ago");
        assert_eq!(age(7200), "2 h ago");
        assert_eq!(age(200000), "2 d ago");
    }
}
