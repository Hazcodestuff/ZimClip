//! Settings export/import bundle. Pure parsing, validation and apply logic; not wired to any UI.
//! Whitelisted keys only. No paths, device ids, monitor index, update state or clips.
use std::path::Path;
pub const HEADER_V1: &str = "ZimClip settings v1";
/// Current export header. v1 files stay readable; v2 adds the closed-set appearance, layout and
/// file-name keys in V2_KEYS. Machine values (paths, devices, monitor, hotkeys, binary files, state)
/// are never part of either version.
pub const HEADER: &str = "ZimClip settings v2";
pub const MAX_BYTES: usize = 4096;
const KEYS: [&str; 11] = [
    "buffer_seconds", "share_default_mb", "large_cards", "close_action", "width", "fps",
    "compatibility", "system_audio", "microphone", "system_gain", "microphone_gain",
];
const V2_KEYS: [&str; 9] = ["layout", "editor_layout", "accent", "theme", "sound", "toast", "ui", "motion", "name_style"];
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// layout.txt, 0 to 4.
    pub layout: Option<u8>,
    /// editor-layout.txt: 1 classic, 2 alternate (the file holds "2" for alternate).
    pub editor_layout: Option<u8>,
    /// appearance.ini accent index 0 to 7.
    pub accent: Option<u8>,
    /// personalization.ini theme, preset themes 0 to 5 only (6 is the custom theme, not exported).
    pub theme: Option<u8>,
    pub sound: Option<bool>,
    pub toast: Option<bool>,
    /// true = polished, false = legacy.
    pub ui_polished: Option<bool>,
    /// true = reduced motion.
    pub motion_reduce: Option<bool>,
    /// name-style.txt 0 to 2 (3 is the custom template, not exported).
    pub name_style: Option<u8>,
    pub buffer_seconds: Option<u32>,
    /// Some(0) means no default share size.
    pub share_default_mb: Option<u32>,
    pub large_cards: Option<bool>,
    pub close_hides: Option<bool>,
    pub width: Option<u32>,
    pub fps: Option<u32>,
    pub compatibility: Option<bool>,
    pub system_audio: Option<bool>,
    pub microphone: Option<bool>,
    pub system_gain: Option<i32>,
    pub microphone_gain: Option<i32>,
}
fn flag(v: &str) -> Result<bool, String> {
    match v { "0" => Ok(false), "1" => Ok(true), _ => Err(format!("expected 0 or 1, got a different value")) }
}
/// Whole-bundle validation. Any problem rejects everything; nothing is written here.
pub fn parse(text: &str) -> Result<Bundle, String> {
    if text.len() > MAX_BYTES {
        return Err("Settings file is too large.".into());
    }
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let v2 = match lines.next() {
        Some(HEADER) => true,
        Some(HEADER_V1) => false,
        Some(h) if h.starts_with("ZimClip settings v") => return Err("This settings file is from a different ZimClip version.".into()),
        _ => return Err("This is not a ZimClip settings file.".into()),
    };
    let mut b = Bundle::default();
    let mut seen: Vec<&str> = Vec::new();
    for l in lines {
        let (k, v) = l.split_once('=').ok_or("A line has no value.")?;
        let (k, v) = (k.trim(), v.trim());
        if !(KEYS.contains(&k) || (v2 && V2_KEYS.contains(&k))) {
            return Err(format!("Unknown setting: {}", k.chars().take(40).collect::<String>()));
        }
        if seen.contains(&k) {
            return Err(format!("Setting listed twice: {k}"));
        }
        seen.push(KEYS.iter().chain(V2_KEYS.iter()).find(|x| **x == k).unwrap());
        let num = || v.parse::<i64>().map_err(|_| format!("{k}: not a number"));
        match k {
            "buffer_seconds" => { let n = num()?; if !(5..=300).contains(&n) { return Err("buffer_seconds must be 5 to 300".into()); } b.buffer_seconds = Some(n as u32) }
            "share_default_mb" => { let n = num()?; if n != 0 && !crate::file_prefs::SHARE_SIZES_MB.contains(&(n as u32)) || n < 0 { return Err("share_default_mb is not a supported size".into()); } b.share_default_mb = Some(n as u32) }
            "large_cards" => b.large_cards = Some(flag(v).map_err(|e| format!("{k}: {e}"))?),
            "close_action" => b.close_hides = Some(match v { "hide" => true, "exit" => false, _ => return Err("close_action must be hide or exit".into()) }),
            "width" => { let n = num()?; if ![854, 1280, 1920].contains(&n) { return Err("width must be 854, 1280 or 1920".into()); } b.width = Some(n as u32) }
            "fps" => { let n = num()?; if ![30, 60].contains(&n) { return Err("fps must be 30 or 60".into()); } b.fps = Some(n as u32) }
            "compatibility" => b.compatibility = Some(flag(v).map_err(|e| format!("{k}: {e}"))?),
            "system_audio" => b.system_audio = Some(flag(v).map_err(|e| format!("{k}: {e}"))?),
            "microphone" => b.microphone = Some(flag(v).map_err(|e| format!("{k}: {e}"))?),
            "system_gain" => { let n = num()?; if !(0..=24).contains(&n) { return Err("system_gain must be 0 to 24".into()); } b.system_gain = Some(n as i32) }
            "microphone_gain" => { let n = num()?; if !(0..=24).contains(&n) { return Err("microphone_gain must be 0 to 24".into()); } b.microphone_gain = Some(n as i32) }
            "layout" => { let n = num()?; if !(0..=4).contains(&n) { return Err("layout must be 0 to 4".into()); } b.layout = Some(n as u8) }
            "editor_layout" => { let n = num()?; if ![1, 2].contains(&n) { return Err("editor_layout must be 1 or 2".into()); } b.editor_layout = Some(n as u8) }
            "accent" => { let n = num()?; if !(0..=7).contains(&n) { return Err("accent must be 0 to 7".into()); } b.accent = Some(n as u8) }
            "theme" => { let n = num()?; if !(0..=5).contains(&n) { return Err("theme must be 0 to 5".into()); } b.theme = Some(n as u8) }
            "sound" => b.sound = Some(flag(v).map_err(|e| format!("{k}: {e}"))?),
            "toast" => b.toast = Some(flag(v).map_err(|e| format!("{k}: {e}"))?),
            "ui" => b.ui_polished = Some(match v { "polished" => true, "legacy" => false, _ => return Err("ui must be polished or legacy".into()) }),
            "motion" => b.motion_reduce = Some(match v { "reduce" => true, "full" => false, _ => return Err("motion must be full or reduce".into()) }),
            _ => { let n = num()?; if !(0..=2).contains(&n) { return Err("name_style must be 0 to 2".into()); } b.name_style = Some(n as u8) }
        }
    }
    if seen.is_empty() { return Err("The settings file has no settings.".into()); }
    Ok(b)
}
/// Export of current values. Capture values come from settings.ini; monitor is left out.
pub fn export(root: &Path) -> String {
    let c = crate::capture_settings::CaptureSettings::load(root);
    let secs = std::fs::read_to_string(root.join("buffer-seconds.txt")).ok().and_then(|s| s.trim().parse::<u32>().ok()).filter(|v| (5..=300).contains(v)).unwrap_or(30);
    let large = std::fs::read_to_string(root.join("library.ini")).map(|s| s.trim() != "large=0").unwrap_or(true);
    let exit = std::fs::read_to_string(root.join("close-action.txt")).map(|s| s.trim() == "exit").unwrap_or(false);
    let a = Appearance::load(root);
    let mut v2 = format!("layout={}\neditor_layout={}\naccent={}\nsound={}\ntoast={}\nui={}\nmotion={}\n", a.layout, a.editor_layout, a.accent, a.sound as u8, a.toast as u8, if a.polished { "polished" } else { "legacy" }, if a.reduce { "reduce" } else { "full" });
    // Custom theme and custom file-name template depend on files this bundle does not carry.
    if a.theme < 6 { v2.push_str(&format!("theme={}\n", a.theme)); }
    if a.name_style < 3 { v2.push_str(&format!("name_style={}\n", a.name_style)); }
    format!("{HEADER}\n{v2}buffer_seconds={secs}\nshare_default_mb={}\nlarge_cards={}\nclose_action={}\nwidth={}\nfps={}\ncompatibility={}\nsystem_audio={}\nmicrophone={}\nsystem_gain={}\nmicrophone_gain={}\n",
        crate::file_prefs::share_default_mb(root).unwrap_or(0), large as u8, if exit { "exit" } else { "hide" },
        c.width, c.fps, c.compatibility as u8, c.system_audio as u8, c.microphone as u8, c.system_gain, c.microphone_gain)
}
/// Effective appearance values exactly as the app reads them today (same defaults and clamps).
struct Appearance { layout: u8, editor_layout: u8, accent: u8, theme: u8, sound: bool, toast: bool, polished: bool, reduce: bool, name_style: u8 }
fn pref_line(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v.to_string()))
}
impl Appearance {
    fn load(root: &Path) -> Appearance {
        let prefs = std::fs::read_to_string(root.join("personalization.ini")).unwrap_or_default();
        let num = |k: &str, d: usize| pref_line(&prefs, k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(d);
        let layout = std::fs::read_to_string(root.join("layout.txt")).ok().and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(0).min(4);
        let accent = std::fs::read_to_string(root.join("appearance.ini")).ok().and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(0) % 8;
        let editor_layout = if std::fs::read_to_string(root.join("editor-layout.txt")).map(|s| s.trim() == "2").unwrap_or(false) { 2 } else { 1 };
        let legacy = pref_line(&prefs, "ui").map(|v| { let v = v.trim().to_string(); v == "legacy" || v == "0" }).unwrap_or(false);
        Appearance {
            layout: layout as u8, editor_layout, accent: accent as u8, theme: (num("theme", 0) % 7) as u8,
            sound: num("sound", 1) != 0, toast: num("toast", 1) != 0, polished: !legacy,
            reduce: pref_line(&prefs, "motion").map(|v| v.trim() == "reduce").unwrap_or(false),
            name_style: crate::file_prefs::name_style(root),
        }
    }
}
/// Replace or append `key=value` lines, keeping every other line (including hotkey) as it is.
fn set_pref_lines(old: &str, set: &[(&str, String)]) -> String {
    let mut lines: Vec<String> = old.lines().map(str::to_string).collect();
    for (k, v) in set {
        match lines.iter_mut().find(|l| l.split_once('=').is_some_and(|(n, _)| n == *k)) {
            Some(l) => *l = format!("{k}={v}"),
            None => lines.push(format!("{k}={v}")),
        }
    }
    let mut t = lines.join("\n");
    t.push('\n');
    t
}
/// One staged file change. None removes the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change { pub file: &'static str, pub content: Option<String> }
pub struct Plan { pub changes: Vec<Change>, pub preview: Vec<String>, pub capture_changed: bool }
/// Compute what would change against the current files. Writes nothing. Unchanged values are skipped.
pub fn plan(root: &Path, b: &Bundle) -> Plan {
    let mut changes = Vec::new();
    let mut preview = Vec::new();
    let mut capture_changed = false;
    let cur = crate::capture_settings::CaptureSettings::load(root);
    let mut c = cur.clone();
    if let Some(v) = b.width { c.width = v }
    if let Some(v) = b.fps { c.fps = v }
    if let Some(v) = b.compatibility { c.compatibility = v }
    if let Some(v) = b.system_audio { c.system_audio = v }
    if let Some(v) = b.microphone { c.microphone = v }
    if let Some(v) = b.system_gain { c.system_gain = v }
    if let Some(v) = b.microphone_gain { c.microphone_gain = v }
    if c != cur {
        capture_changed = true;
        for (n, a, z) in [("Width", cur.width.to_string(), c.width.to_string()), ("FPS", cur.fps.to_string(), c.fps.to_string()), ("Compatibility capture", cur.compatibility.to_string(), c.compatibility.to_string()), ("System audio", cur.system_audio.to_string(), c.system_audio.to_string()), ("Microphone", cur.microphone.to_string(), c.microphone.to_string()), ("System gain", cur.system_gain.to_string(), c.system_gain.to_string()), ("Microphone gain", cur.microphone_gain.to_string(), c.microphone_gain.to_string())] {
            if a != z { preview.push(format!("{n}: {a} -> {z}")); }
        }
        changes.push(Change { file: "settings.ini", content: Some(c.text()) });
    }
    if let Some(v) = b.buffer_seconds {
        let old = std::fs::read_to_string(root.join("buffer-seconds.txt")).ok().and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(30);
        if old != v { preview.push(format!("Clip length: {old}s -> {v}s")); changes.push(Change { file: "buffer-seconds.txt", content: Some(v.to_string()) }); capture_changed = true; }
    }
    if let Some(v) = b.share_default_mb {
        let old = crate::file_prefs::share_default_mb(root).unwrap_or(0);
        if old != v { preview.push(format!("Default export target: {old} -> {v} MB (0 = none)")); changes.push(Change { file: "share-default.txt", content: (v != 0).then(|| v.to_string()) }); }
    }
    if let Some(v) = b.large_cards {
        let old = std::fs::read_to_string(root.join("library.ini")).map(|s| s.trim() != "large=0").unwrap_or(true);
        if old != v { preview.push(format!("Large clip cards: {old} -> {v}")); changes.push(Change { file: "library.ini", content: Some(if v { "large=1" } else { "large=0" }.into()) }); }
    }
    if let Some(v) = b.close_hides {
        let old = !std::fs::read_to_string(root.join("close-action.txt")).map(|s| s.trim() == "exit").unwrap_or(false);
        if old != v { preview.push(format!("Close button: {} -> {}", if old { "hide" } else { "exit" }, if v { "hide" } else { "exit" })); changes.push(Change { file: "close-action.txt", content: (!v).then(|| "exit".to_string()) }); }
    }
    let a = Appearance::load(root);
    let onoff = |v: bool| if v { "on" } else { "off" };
    if let Some(v) = b.layout { if a.layout != v { preview.push(format!("Window layout: {} -> {v}", a.layout)); changes.push(Change { file: "layout.txt", content: Some(v.to_string()) }); } }
    if let Some(v) = b.editor_layout { if a.editor_layout != v { preview.push(format!("Editor layout: {} -> {v}", a.editor_layout)); changes.push(Change { file: "editor-layout.txt", content: (v == 2).then(|| "2".to_string()) }); } }
    if let Some(v) = b.accent { if a.accent != v { preview.push(format!("Accent colour: {} -> {v}", a.accent)); changes.push(Change { file: "appearance.ini", content: Some(v.to_string()) }); } }
    if let Some(v) = b.name_style { if a.name_style != v { preview.push(format!("File names style: {} -> {v}", a.name_style)); changes.push(Change { file: "name-style.txt", content: (v != 0).then(|| v.to_string()) }); } }
    let mut set: Vec<(&str, String)> = Vec::new();
    if let Some(v) = b.theme { if a.theme != v { preview.push(format!("Theme: {} -> {v}", a.theme)); set.push(("theme", v.to_string())); } }
    if let Some(v) = b.sound { if a.sound != v { preview.push(format!("Sound: {} -> {}", onoff(a.sound), onoff(v))); set.push(("sound", (v as u8).to_string())); } }
    if let Some(v) = b.toast { if a.toast != v { preview.push(format!("Notifications: {} -> {}", onoff(a.toast), onoff(v))); set.push(("toast", (v as u8).to_string())); } }
    if let Some(v) = b.ui_polished { if a.polished != v { preview.push(format!("Interface: {} -> {}", if a.polished { "polished" } else { "legacy" }, if v { "polished" } else { "legacy" })); set.push(("ui", if v { "polished" } else { "legacy" }.to_string())); } }
    if let Some(v) = b.motion_reduce { if a.reduce != v { preview.push(format!("Motion: {} -> {}", if a.reduce { "reduce" } else { "full" }, if v { "reduce" } else { "full" })); set.push(("motion", if v { "reduce" } else { "full" }.to_string())); } }
    if !set.is_empty() {
        let old = std::fs::read_to_string(root.join("personalization.ini")).unwrap_or_default();
        changes.push(Change { file: "personalization.ini", content: Some(set_pref_lines(&old, &set)) });
    }
    Plan { changes, preview, capture_changed }
}
/// Temp file, sync_all, then rename (Windows: MoveFileEx replace with write-through). Used for
/// every file this module changes, so each settings file is flushed before the next step and
/// before the complete mark. Limits: deleting a file has no flush, and no directory flush is done;
/// real durability after power loss still depends on the file system and drive honouring flushes.
fn durable_write(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut name = path.file_name().ok_or("bad path")?.to_os_string();
    name.push(".part");
    let tmp = path.with_file_name(name);
    let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    f.write_all(text.as_bytes()).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
    drop(f);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH)
        } == 0 {
            let e = std::io::Error::last_os_error().to_string();
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    std::fs::rename(&tmp, path).map_err(|e| { let _ = std::fs::remove_file(&tmp); e.to_string() })
}
pub const JOURNAL: &str = "settings-apply.journal";
const FILES: [&str; 10] = ["settings.ini", "buffer-seconds.txt", "share-default.txt", "library.ini", "close-action.txt", "layout.txt", "editor-layout.txt", "appearance.ini", "personalization.ini", "name-style.txt"];
fn hex(s: &str) -> String { s.bytes().map(|b| format!("{b:02x}")).collect() }
fn unhex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 { return None; }
    let v: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect();
    String::from_utf8(v?).ok()
}
fn write_or_remove(p: &Path, c: &Option<String>) -> Result<(), String> {
    match c {
        Some(t) => durable_write(p, t),
        None => match std::fs::remove_file(p) { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(e) => Err(e.to_string()) },
    }
}
fn restore(root: &Path, saved: &[(String, Option<String>)]) -> Result<(), String> {
    let mut err = None;
    for (f, old) in saved.iter().rev() {
        if let Err(e) = write_or_remove(&root.join(f), old) { err = Some(e); }
    }
    err.map_or(Ok(()), Err)
}
const COMPLETE: &str = "ZimClip settings journal complete\n";
pub const QUARANTINE: &str = "settings-apply.journal.corrupt";
#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    /// No interrupted import.
    Clean,
    /// An interrupted import was found and every original value was put back.
    Restored,
    /// Could not restore, or the journal is unreadable. Settings files are left as they are, the
    /// evidence is kept, and imports stay blocked. The message is user-facing.
    Blocked(String),
}
/// Run once at startup, before any settings read or capture start. An interrupted apply leaves the
/// journal, which holds every original value: restore them and delete the journal. An unreadable
/// or unrecognised journal may mean a partial apply, so it is moved aside (never deleted), no
/// settings file is touched, and imports are blocked while it exists.
pub fn recover(root: &Path) -> Recovery {
    if root.join(QUARANTINE).exists() { return Recovery::Blocked(blocked_msg()); }
    let jp = root.join(JOURNAL);
    if !jp.exists() { return Recovery::Clean; }
    if std::fs::read_to_string(&jp).map(|t| t == COMPLETE).unwrap_or(false) {
        // The import finished; only the cleanup was interrupted. Keep the new settings.
        return match std::fs::remove_file(&jp) { Ok(()) => Recovery::Clean, Err(_) => Recovery::Blocked(blocked_msg()) };
    }
    let parsed = std::fs::read_to_string(&jp).ok().and_then(|text| {
        if text.lines().next() != Some("ZimClip settings journal v1") { return None; }
        let mut saved = Vec::new();
        for l in text.lines().skip(1) {
            let mut p = l.split('\t');
            match (p.next(), p.next(), p.next()) {
                (Some(f), Some("absent"), None) if FILES.contains(&f) => saved.push((f.to_string(), None)),
                (Some(f), Some("present"), Some(h)) if FILES.contains(&f) => saved.push((f.to_string(), Some(unhex(h)?))),
                _ => return None,
            }
        }
        Some(saved)
    });
    match parsed {
        None => {
            let _ = std::fs::rename(&jp, root.join(QUARANTINE));
            Recovery::Blocked(blocked_msg())
        }
        Some(saved) => match restore(root, &saved) {
            Ok(()) => match std::fs::remove_file(&jp) { Ok(()) => Recovery::Restored, Err(_) => Recovery::Blocked(blocked_msg()) },
            Err(_) => Recovery::Blocked(blocked_msg()),
        },
    }
}
/// Journal goes through durable_write (temp, flush, rename), so a journal that exists is
/// complete, and it is on disk before the first settings file changes.
fn write_journal(root: &Path, text: &str) -> Result<(), String> {
    durable_write(&root.join(JOURNAL), text)
}
/// User-confirmed way out of Blocked. It does NOT change settings and does not claim they were
/// restored: the unreadable record is moved to settings-apply.journal.resolved-N (evidence kept)
/// and imports work again. The caller's dialog must say "settings stay as they are now; the
/// unfinished import record is kept". Refused when a readable journal still exists (restore that
/// first) or there is nothing to resolve.
pub fn resolve_lock(root: &Path) -> Result<String, String> {
    if root.join(JOURNAL).exists() { return Err("An unfinished import still has to be undone. Restart ZimClip to retry.".into()); }
    let q = root.join(QUARANTINE);
    if !q.exists() { return Err("Nothing to resolve.".into()); }
    for n in 1..100 {
        let dst = root.join(format!("settings-apply.journal.resolved-{n}"));
        if !dst.exists() {
            std::fs::rename(&q, &dst).map_err(|e| e.to_string())?;
            return Ok(format!("Imports are on again. This step did not change any settings, which may still be a mix of old and new values. The unfinished record was kept as {}.", dst.file_name().unwrap().to_string_lossy()));
        }
    }
    Err("Too many kept records.".into())
}
fn blocked_msg() -> String {
    "An earlier settings import did not finish and could not be undone automatically. Some settings may be partly changed, a mix of old and new values. Importing is turned off until this is resolved.".into()
}
/// Apply a validated plan. The journal (originals, inside the app folder) is written first and
/// removed only after every file landed. A failed write restores every file already changed;
/// a crash is repaired by recover() at the next start.
pub fn apply(root: &Path, changes: &[Change]) -> Result<(), String> {
    match recover(root) { Recovery::Blocked(m) => return Err(m), _ => {} }
    if changes.iter().any(|c| !FILES.contains(&c.file)) { return Err("Import stopped, your settings are unchanged: unsupported file".into()); }
    let saved: Vec<(String, Option<String>)> = changes.iter().map(|c| (c.file.to_string(), std::fs::read_to_string(root.join(c.file)).ok())).collect();
    let mut j = String::from("ZimClip settings journal v1\n");
    for (f, old) in &saved {
        match old { Some(t) => j.push_str(&format!("{f}\tpresent\t{}\n", hex(t))), None => j.push_str(&format!("{f}\tabsent\n")) }
    }
    write_journal(root, &j).map_err(|e| format!("Import stopped, your settings are unchanged: {e}"))?;
    for ch in changes {
        if let Err(e) = write_or_remove(&root.join(ch.file), &ch.content) {
            return Err(undo_after_failure(root, &saved, &e));
        }
    }
    // Mark the import finished (flushed) before deleting, so a failed delete can never make the
    // next start undo a good import. If the mark cannot be written, undo instead of guessing.
    if let Err(e) = write_journal(root, COMPLETE) {
        return Err(undo_after_failure(root, &saved, &e));
    }
    match std::fs::remove_file(root.join(JOURNAL)) {
        Ok(()) => Ok(()),
        // Settings are applied; a marked-complete journal is harmless and cleaned at next start.
        Err(_) => Ok(()),
    }
}
fn undo_after_failure(root: &Path, saved: &[(String, Option<String>)], cause: &str) -> String {
    match restore(root, saved) {
        Ok(()) => match std::fs::remove_file(root.join(JOURNAL)) {
            Ok(()) => format!("Import stopped, your settings are unchanged: {cause}"),
            Err(_) => format!("Import stopped and settings were put back: {cause}. A record file could not be removed and will be retried at next start."),
        },
        Err(_) => format!("Import stopped part-way and the undo failed, so some settings may be partly changed ({cause}). ZimClip keeps a record and will retry the undo at next start. Importing is off until then."),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dir(n: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("zim-bundle-{n}-{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }
    #[test]
    fn export_parses_back_and_has_no_machine_values() {
        let d = dir("exp");
        let t = export(&d);
        assert!(parse(&t).is_ok());
        for bad in ["monitor", "video", "device", ":\\", "/"] { assert!(!t.contains(bad), "{bad}"); }
        assert!(plan(&d, &parse(&t).unwrap()).changes.is_empty());
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn rejects_bad_bundles_whole() {
        for t in ["", "hello", "ZimClip settings v3\nfps=30", "ZimClip settings v1", "ZimClip settings v1\nfps=30\nfps=60", "ZimClip settings v1\nfps=45", "ZimClip settings v1\nvideo_folder=C:\\x", "ZimClip settings v1\nfps=30\nwidth=999", "ZimClip settings v1\nclose_action=maybe", "ZimClip settings v1\nbuffer_seconds=301", "ZimClip settings v1\nshare_default_mb=7", "ZimClip settings v1\nfps"] {
            assert!(parse(t).is_err(), "{t}");
        }
        assert!(parse(&format!("{HEADER}\n{}", "fps=30\n".repeat(1000))).is_err());
    }
    #[test]
    fn plan_previews_and_apply_changes_only_named_values_keeping_monitor() {
        let d = dir("apply");
        crate::capture_settings::CaptureSettings { monitor: 2, ..Default::default() }.save(&d).unwrap();
        let b = parse("ZimClip settings v1\nfps=60\nbuffer_seconds=90\nclose_action=exit\nshare_default_mb=25").unwrap();
        let p = plan(&d, &b);
        assert!(p.capture_changed && p.preview.len() == 4);
        assert!(!d.join("buffer-seconds.txt").exists(), "plan must not write");
        apply(&d, &p.changes).unwrap();
        let c = crate::capture_settings::CaptureSettings::load(&d);
        assert_eq!((c.monitor, c.fps), (2, 60));
        assert_eq!(std::fs::read_to_string(d.join("close-action.txt")).unwrap(), "exit");
        assert!(!d.join(JOURNAL).exists());
        let back = parse("ZimClip settings v1\nclose_action=hide\nshare_default_mb=0").unwrap();
        apply(&d, &plan(&d, &back).changes).unwrap();
        assert!(!d.join("close-action.txt").exists() && !d.join("share-default.txt").exists());
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn failed_write_rolls_back_earlier_changes() {
        let d = dir("roll");
        std::fs::write(d.join("buffer-seconds.txt"), "30").unwrap();
        // library.ini is a directory, so the second write fails.
        std::fs::create_dir(d.join("library.ini")).unwrap();
        let ch = vec![Change { file: "buffer-seconds.txt", content: Some("90".into()) }, Change { file: "library.ini", content: Some("large=0".into()) }];
        assert!(apply(&d, &ch).is_err());
        assert_eq!(std::fs::read_to_string(d.join("buffer-seconds.txt")).unwrap(), "30");
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn crash_mid_apply_is_repaired_by_recover() {
        let d = dir("crash");
        std::fs::write(d.join("buffer-seconds.txt"), "30").unwrap();
        std::fs::write(d.join("close-action.txt"), "exit").unwrap();
        let j = format!("ZimClip settings journal v1\nbuffer-seconds.txt\tpresent\t{}\nclose-action.txt\tpresent\t{}\nshare-default.txt\tabsent\n", hex("30"), hex("exit"));
        std::fs::write(d.join(JOURNAL), j).unwrap();
        std::fs::write(d.join("buffer-seconds.txt"), "90").unwrap();
        std::fs::remove_file(d.join("close-action.txt")).unwrap();
        std::fs::write(d.join("share-default.txt"), "25").unwrap();
        assert_eq!(recover(&d), Recovery::Restored);
        assert_eq!(std::fs::read_to_string(d.join("buffer-seconds.txt")).unwrap(), "30");
        assert_eq!(std::fs::read_to_string(d.join("close-action.txt")).unwrap(), "exit");
        assert!(!d.join("share-default.txt").exists() && !d.join(JOURNAL).exists());
        assert_eq!(recover(&d), Recovery::Clean);
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn corrupt_journal_is_quarantined_and_blocks_import() {
        let d = dir("corrupt");
        std::fs::write(d.join("buffer-seconds.txt"), "90").unwrap();
        for bad in ["ZimClip settings journal v1\nvideo-folder.txt\tabsent\n", "garbage", "ZimClip settings journal v1\nbuffer-seconds.txt\tpresent\tzz\n"] {
            std::fs::write(d.join(JOURNAL), bad).unwrap();
            assert!(matches!(recover(&d), Recovery::Blocked(_)));
            assert!(!d.join(JOURNAL).exists() && d.join(QUARANTINE).exists());
            assert_eq!(std::fs::read_to_string(d.join("buffer-seconds.txt")).unwrap(), "90");
            let ch = vec![Change { file: "buffer-seconds.txt", content: Some("30".into()) }];
            assert!(apply(&d, &ch).unwrap_err().contains("Importing is turned off"));
            assert_eq!(std::fs::read_to_string(d.join("buffer-seconds.txt")).unwrap(), "90");
            std::fs::remove_file(d.join(QUARANTINE)).unwrap();
        }
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn restore_failure_keeps_journal_and_blocks() {
        let d = dir("restorefail");
        let j = format!("ZimClip settings journal v1\nlibrary.ini\tpresent\t{}\n", hex("large=1"));
        std::fs::write(d.join(JOURNAL), j).unwrap();
        std::fs::create_dir(d.join("library.ini")).unwrap(); // restore cannot write here
        assert!(matches!(recover(&d), Recovery::Blocked(_)));
        assert!(d.join(JOURNAL).exists());
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn resolve_lock_keeps_evidence_and_leaves_settings() {
        let d = dir("resolve");
        std::fs::write(d.join("buffer-seconds.txt"), "90").unwrap();
        assert!(resolve_lock(&d).is_err());
        std::fs::write(d.join(JOURNAL), "garbage").unwrap();
        assert!(matches!(recover(&d), Recovery::Blocked(_)));
        let msg = resolve_lock(&d).unwrap();
        assert!(msg.contains("did not change") && msg.contains("mix") && !msg.to_lowercase().contains("restored"));
        assert_eq!(std::fs::read_to_string(d.join("settings-apply.journal.resolved-1")).unwrap(), "garbage");
        assert_eq!(recover(&d), Recovery::Clean);
        let ch = vec![Change { file: "buffer-seconds.txt", content: Some("30".into()) }];
        apply(&d, &ch).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("buffer-seconds.txt")).unwrap(), "30");
        std::fs::write(d.join(JOURNAL), format!("ZimClip settings journal v1\nbuffer-seconds.txt\tpresent\t{}\n", hex("1"))).unwrap();
        assert!(resolve_lock(&d).is_err());
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn leftover_complete_journal_keeps_new_settings() {
        let d = dir("complete");
        std::fs::write(d.join("buffer-seconds.txt"), "30").unwrap();
        apply(&d, &[Change { file: "buffer-seconds.txt", content: Some("90".into()) }]).unwrap();
        // Simulate a failed delete after the complete mark.
        std::fs::write(d.join(JOURNAL), COMPLETE).unwrap();
        assert_eq!(recover(&d), Recovery::Clean);
        assert_eq!(std::fs::read_to_string(d.join("buffer-seconds.txt")).unwrap(), "90");
        assert!(!d.join(JOURNAL).exists());
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn write_failure_and_failed_undo_reports_partial_and_keeps_journal() {
        let d = dir("partial");
        std::fs::write(d.join("buffer-seconds.txt"), "30").unwrap();
        std::fs::write(d.join("share-default.txt"), "25").unwrap();
        // library.ini is a directory: the second write fails, and the undo of it cannot succeed either
        // when the journal says it was a file.
        std::fs::create_dir(d.join("library.ini")).unwrap();
        std::fs::write(d.join("library.ini").join("x"), "x").unwrap();
        let saved = vec![("buffer-seconds.txt".to_string(), Some("30".to_string())), ("library.ini".to_string(), Some("large=1".to_string()))];
        let msg = undo_after_failure(&d, &saved, "disk error");
        assert!(msg.contains("partly changed") && !msg.contains("unchanged"), "{msg}");
    }
    #[test]
    fn max_bundle_lists_all_eleven_changes_and_negative_gain_names_the_key() {
        let d = dir("max");
        let good = "ZimClip settings v1\nbuffer_seconds=60\nshare_default_mb=25\nlarge_cards=0\nclose_action=exit\nwidth=1920\nfps=60\ncompatibility=0\nsystem_audio=0\nmicrophone=1\nsystem_gain=6\nmicrophone_gain=6\n";
        let p = plan(&d, &parse(good).unwrap());
        assert_eq!(p.preview.len(), 11, "{:?}", p.preview);
        assert!(p.capture_changed);
        let bad = good.replace("microphone_gain=6", "microphone_gain=-6");
        assert_eq!(parse(&bad).unwrap_err(), "microphone_gain must be 0 to 24");
        assert!(parse(&good.replace("system_gain=6", "system_gain=40")).unwrap_err().contains("system_gain"));
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn v1_files_still_parse_and_v2_keys_are_rejected_in_v1() {
        let b = parse("ZimClip settings v1\nfps=60\nbuffer_seconds=90").unwrap();
        assert_eq!((b.fps, b.buffer_seconds, b.layout), (Some(60), Some(90), None));
        assert!(parse("ZimClip settings v1\nlayout=1").is_err());
        assert!(parse("ZimClip settings v2\nlayout=1\nfps=30").is_ok());
    }
    #[test]
    fn v2_validators_are_closed_sets() {
        for bad in ["layout=5", "layout=-1", "layout=x", "editor_layout=0", "editor_layout=3", "accent=8", "theme=6", "theme=7", "sound=2", "toast=yes", "ui=old", "ui=0", "motion=none", "name_style=3", "name_style=-1", "hotkey=1", "video_folder=x", "icon=x", "layout=1\nlayout=2"] {
            assert!(parse(&format!("ZimClip settings v2\n{bad}")).is_err(), "{bad}");
        }
        let ok = parse("ZimClip settings v2\nlayout=4\neditor_layout=2\naccent=7\ntheme=5\nsound=0\ntoast=0\nui=legacy\nmotion=reduce\nname_style=2").unwrap();
        assert_eq!((ok.layout, ok.editor_layout, ok.accent, ok.theme, ok.name_style), (Some(4), Some(2), Some(7), Some(5), Some(2)));
    }
    #[test]
    fn v2_export_roundtrips_and_leaves_hotkey_and_machine_files_alone() {
        let d = dir("v2");
        std::fs::write(d.join("layout.txt"), "2").unwrap();
        std::fs::write(d.join("editor-layout.txt"), "2").unwrap();
        std::fs::write(d.join("appearance.ini"), "3").unwrap();
        std::fs::write(d.join("personalization.ini"), "theme=1\nhotkey=1095\nsound=1\ntoast=1\nui=polished\nmotion=full\n").unwrap();
        std::fs::write(d.join("name-style.txt"), "2").unwrap();
        std::fs::write(d.join("video-folder.txt"), "D:\\clips").unwrap();
        let t = export(&d);
        assert!(t.starts_with(HEADER) && t.contains("layout=2") && t.contains("editor_layout=2") && t.contains("accent=3") && t.contains("theme=1") && t.contains("name_style=2"));
        assert!(!t.contains("hotkey") && !t.contains("clips") && !t.contains("1095"));
        assert!(plan(&d, &parse(&t).unwrap()).changes.is_empty());
        // Apply to a second folder with different values and its own hotkey.
        let e = dir("v2b");
        std::fs::write(e.join("personalization.ini"), "theme=0\nhotkey=999\nsound=0\ntoast=1\nui=legacy\nmotion=reduce\n").unwrap();
        let p = plan(&e, &parse(&t).unwrap());
        // Appearance-only differences: Theme, Sound, Interface and Motion are previewed, and the
        // capture settings are untouched because the capture values here already match.
        for key in ["Theme", "Sound", "Interface", "Motion", "Window layout", "Editor layout", "Accent colour", "File names style"] {
            assert!(p.preview.iter().any(|l| l.starts_with(key)), "{key}");
        }
        assert!(!p.capture_changed);
        assert!(!p.changes.iter().any(|c| c.file == "settings.ini"));
        apply(&e, &p.changes).unwrap();
        let pi = std::fs::read_to_string(e.join("personalization.ini")).unwrap();
        assert!(pi.contains("hotkey=999") && pi.contains("theme=1") && pi.contains("sound=1") && pi.contains("ui=polished") && pi.contains("motion=full"));
        assert_eq!(std::fs::read_to_string(e.join("layout.txt")).unwrap(), "2");
        assert_eq!(std::fs::read_to_string(e.join("editor-layout.txt")).unwrap(), "2");
        assert!(!e.join("video-folder.txt").exists() && !e.join("audio-devices.txt").exists());
        assert!(plan(&e, &parse(&t).unwrap()).changes.is_empty());
        // Back to classic editor layout removes the file, as the app treats absent as classic.
        let back = parse("ZimClip settings v2\neditor_layout=1\nname_style=0").unwrap();
        apply(&e, &plan(&e, &back).changes).unwrap();
        assert!(!e.join("editor-layout.txt").exists() && !e.join("name-style.txt").exists());
        std::fs::remove_dir_all(&d).ok();
        std::fs::remove_dir_all(&e).ok();
    }
    #[test]
    fn custom_theme_and_template_are_not_exported() {
        let d = dir("v2c");
        std::fs::write(d.join("personalization.ini"), "theme=6\nhotkey=1\n").unwrap();
        std::fs::write(d.join("name-style.txt"), "3").unwrap();
        let t = export(&d);
        assert!(!t.contains("theme=") && !t.contains("name_style=3"));
        assert!(parse(&t).is_ok());
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn interrupted_v2_apply_is_restored_on_next_start() {
        let d = dir("v2r");
        std::fs::write(d.join("layout.txt"), "1").unwrap();
        std::fs::write(d.join("personalization.ini"), "theme=0\nhotkey=7\n").unwrap();
        let p = plan(&d, &parse("ZimClip settings v2\nlayout=3\ntheme=2\nui=legacy").unwrap());
        // Simulate a crash: journal written, first file changed, no complete mark.
        let saved: Vec<(String, Option<String>)> = p.changes.iter().map(|c| (c.file.to_string(), std::fs::read_to_string(d.join(c.file)).ok())).collect();
        let mut j = String::from("ZimClip settings journal v1\n");
        for (f, old) in &saved { match old { Some(t) => j.push_str(&format!("{f}\tpresent\t{}\n", hex(t))), None => j.push_str(&format!("{f}\tabsent\n")) } }
        std::fs::write(d.join(JOURNAL), j).unwrap();
        std::fs::write(d.join("layout.txt"), "3").unwrap();
        std::fs::write(d.join("personalization.ini"), "theme=2\nhotkey=7\nui=legacy\n").unwrap();
        assert_eq!(recover(&d), Recovery::Restored);
        assert_eq!(std::fs::read_to_string(d.join("layout.txt")).unwrap(), "1");
        assert_eq!(std::fs::read_to_string(d.join("personalization.ini")).unwrap(), "theme=0\nhotkey=7\n");
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn old_journal_listing_only_v1_files_still_recovers() {
        let d = dir("v1j");
        std::fs::write(d.join("close-action.txt"), "exit").unwrap();
        let j = format!("ZimClip settings journal v1\nclose-action.txt\tabsent\n");
        std::fs::write(d.join(JOURNAL), j).unwrap();
        assert_eq!(recover(&d), Recovery::Restored);
        assert!(!d.join("close-action.txt").exists());
        std::fs::remove_dir_all(&d).ok();
    }
}
