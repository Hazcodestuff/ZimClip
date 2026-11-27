//! Small preference file shared by the editor and Settings.
/// True when the user ticked "Don't show this confirmation again" for clip deletes.
pub fn delete_confirm_skipped(prefs: &std::path::Path) -> bool {
    std::fs::read_to_string(prefs)
        .unwrap_or_default()
        .lines()
        .any(|s| s.trim() == "skip_delete_confirmation=1")
}
/// Turn the delete-confirmation skip on or off. Other lines in the file are kept, and the
/// write goes through a temporary file so a failure cannot truncate the preferences.
pub fn set_delete_confirm_skipped(prefs: &std::path::Path, skip: bool) -> std::io::Result<()> {
    let old = std::fs::read_to_string(prefs).unwrap_or_default();
    let mut out: Vec<String> = old
        .lines()
        .filter(|l| !l.trim().starts_with("skip_delete_confirmation="))
        .map(str::to_string)
        .collect();
    if skip {
        out.push("skip_delete_confirmation=1".into());
    }
    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    let tmp = prefs.with_extension("ini.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, prefs)
}
/// True when the window close button hides ZimClip to the tray (the default). When the user turns
/// it off, closing the window exits the app, so recording stops. Stored as "exit" in close-action.txt.
pub fn close_hides_to_tray(dir: &std::path::Path) -> bool {
    std::fs::read_to_string(dir.join("close-action.txt"))
        .map(|s| s.trim() != "exit")
        .unwrap_or(true)
}
pub fn set_close_hides_to_tray(dir: &std::path::Path, hide: bool) -> Result<(), String> {
    let path = dir.join("close-action.txt");
    if hide {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        crate::save_folder::atomic_text(&path, "exit")
    }
}
pub fn close_label(hide: bool) -> &'static str {
    if hide { "Close button: hide to tray" } else { "Close button: exit app" }
}
/// Sizes offered as the default export target. Only these are stored.
pub const SHARE_SIZES_MB: [u32; 5] = [8, 10, 20, 25, 50];
/// Default export target in MB, or None when the user chose none (the editor keeps Fast trim).
pub fn share_default_mb(dir: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(dir.join("share-default.txt"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|v| SHARE_SIZES_MB.contains(v))
}
pub fn set_share_default_mb(dir: &std::path::Path, mb: Option<u32>) -> Result<(), String> {
    let path = dir.join("share-default.txt");
    match mb {
        Some(v) if SHARE_SIZES_MB.contains(&v) => crate::save_folder::atomic_text(&path, &v.to_string()),
        Some(_) => Err("Unsupported share size".into()),
        None => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}
/// Button text for the Settings row.
pub fn share_label(dir: Option<&std::path::Path>) -> String {
    match dir.and_then(share_default_mb) {
        Some(mb) => format!("Share size default: {mb} MB"),
        None => "Share size default: none".into(),
    }
}
/// Next value when the Settings button is pressed: none, then each size, then back to none.
pub fn next_share_default(current: Option<u32>) -> Option<u32> {
    match current {
        None => Some(SHARE_SIZES_MB[0]),
        Some(v) => SHARE_SIZES_MB.iter().position(|s| *s == v).and_then(|i| SHARE_SIZES_MB.get(i + 1)).copied(),
    }
}
pub fn prefs_file() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("file-actions.ini")))
        .unwrap_or_else(|| std::path::PathBuf::from("file-actions.ini"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_action_defaults_to_tray_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("zc-close-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(close_hides_to_tray(&dir));
        set_close_hides_to_tray(&dir, false).unwrap();
        assert!(!close_hides_to_tray(&dir));
        set_close_hides_to_tray(&dir, true).unwrap();
        assert!(close_hides_to_tray(&dir));
        set_close_hides_to_tray(&dir, true).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn share_default_round_trips_and_rejects_other_sizes() {
        let dir = std::env::temp_dir().join(format!("zc-share-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(share_default_mb(&dir), None);
        set_share_default_mb(&dir, Some(25)).unwrap();
        assert_eq!(share_default_mb(&dir), Some(25));
        assert!(set_share_default_mb(&dir, Some(7)).is_err());
        std::fs::write(dir.join("share-default.txt"), "7").unwrap();
        assert_eq!(share_default_mb(&dir), None);
        set_share_default_mb(&dir, None).unwrap();
        set_share_default_mb(&dir, None).unwrap();
        assert_eq!(share_default_mb(&dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn share_default_cycles_through_every_size_and_back_to_none() {
        let mut v = None;
        let mut seen = Vec::new();
        for _ in 0..SHARE_SIZES_MB.len() {
            v = next_share_default(v);
            seen.push(v.unwrap());
        }
        assert_eq!(seen, SHARE_SIZES_MB.to_vec());
        assert_eq!(next_share_default(v), None);
    }
    #[test]
    fn skip_flag_round_trips_and_keeps_other_lines() {
        let dir = std::env::temp_dir().join(format!("zc-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("file-actions.ini");
        std::fs::write(&f, "other=1\n").unwrap();
        assert!(!delete_confirm_skipped(&f));
        set_delete_confirm_skipped(&f, true).unwrap();
        assert!(delete_confirm_skipped(&f));
        set_delete_confirm_skipped(&f, true).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "other=1\nskip_delete_confirmation=1\n");
        set_delete_confirm_skipped(&f, false).unwrap();
        assert!(!delete_confirm_skipped(&f));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "other=1\n");
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Clip file name styles for new saves. 0 keeps the original "clip-<number>" names.
pub const NAME_STYLES: u8 = 3;
/// Style value that means "use the saved custom template" (name-template.txt).
pub const STYLE_TEMPLATE: u8 = 3;
pub fn name_style(dir: &std::path::Path) -> u8 {
    std::fs::read_to_string(dir.join("name-style.txt"))
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .filter(|v| *v < NAME_STYLES || (*v == STYLE_TEMPLATE && template(dir).is_some()))
        .unwrap_or(0)
}
/// The saved custom template, when one exists and is still valid.
pub fn template(dir: &std::path::Path) -> Option<String> {
    let t = std::fs::read_to_string(dir.join("name-template.txt")).ok()?;
    let t = t.trim().to_string();
    check_template(&t, "ZimClip").ok().map(|_| t)
}
/// Saves a validated template and switches to it. If the second write fails the
/// first one is put back, so a failure leaves both files as they were.
pub fn set_template(dir: &std::path::Path, text: &str, app: &str) -> Result<(), String> {
    check_template(text, app)?;
    let tpath = dir.join("name-template.txt");
    let old_t = std::fs::read_to_string(&tpath).ok();
    crate::save_folder::atomic_text(&tpath, text.trim())?;
    if let Err(e) = set_name_style(dir, STYLE_TEMPLATE) {
        match old_t {
            Some(o) => {
                let _ = crate::save_folder::atomic_text(&tpath, &o);
            }
            None => {
                let _ = std::fs::remove_file(&tpath);
            }
        }
        return Err(e);
    }
    Ok(())
}
pub const TEMPLATE_MAX: usize = 60;
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];
/// Renders a template into a file name without extension, or says in plain words why not.
/// Tokens: {date} 2026-11-27, {time} 143005 (local), {app} the app name made file-safe.
/// Literal text may use letters, digits, - and _ only. Nothing is dropped or cut silently.
pub fn render_template(t: &str, app: &str, tm: (u16, u16, u16, u16, u16, u16)) -> Result<String, String> {
    let (y, mo, d, h, mi, s) = tm;
    let t = t.trim();
    if t.is_empty() {
        return Err("Type a template, or pick a preset.".into());
    }
    let mut out = String::new();
    let mut chars = t.chars();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                for n in chars.by_ref() {
                    if n == '}' {
                        closed = true;
                        break;
                    }
                    if n == '{' {
                        break;
                    }
                    name.push(n);
                }
                if !closed {
                    return Err("A { has no closing }.".into());
                }
                match name.as_str() {
                    "date" => out.push_str(&format!("{y:04}-{mo:02}-{d:02}")),
                    "time" => out.push_str(&format!("{h:02}{mi:02}{s:02}")),
                    "app" => out.push_str(&clean_app_name(app)),
                    other => {
                        let shown: String = other.chars().filter(|c| !c.is_control()).take(12).collect();
                        return Err(format!("Unknown token {{{shown}}}. Use {{date}}, {{time}} or {{app}}."));
                    }
                }
            }
            '}' => return Err("A } has no matching {.".into()),
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' => out.push(c),
            ' ' => return Err("Spaces are not allowed. Use - or _.".into()),
            c => {
                let shown = if c.is_control() { "That character".to_string() } else { format!("'{c}'") };
                return Err(format!("{shown} is not allowed. Use letters, digits, - and _."));
            }
        }
    }
    let n = out.chars().count();
    if n > TEMPLATE_MAX {
        return Err(format!("That makes a {n}-character name. The limit is {TEMPLATE_MAX}."));
    }
    if RESERVED.contains(&out.to_ascii_lowercase().as_str()) {
        return Err(format!("{out} is a reserved Windows name. Add more text."));
    }
    Ok(out)
}
/// Validates with the real app name and with the longest app name, so a later rename
/// cannot turn a saved template into one that fails at save time.
pub fn check_template(t: &str, app: &str) -> Result<(), String> {
    let tm = (2026, 11, 27, 14, 30, 5);
    render_template(t, "x".repeat(24).as_str(), tm)
        .and_then(|_| render_template(t, app, tm))
        .map(|_| ())
}
/// Base name for a new save. The second value is a note for the log when the saved
/// template could not be used and the date-time style was used instead.
pub fn save_base(dir: &std::path::Path, app: &str, tm: (u16, u16, u16, u16, u16, u16), millis: u128) -> (String, Option<String>) {
    let style = name_style(dir);
    if style == STYLE_TEMPLATE {
        if let Some(t) = template(dir) {
            return match render_template(&t, app, tm) {
                Ok(b) => (b, None),
                Err(e) => (clip_base(2, app, tm, millis), Some(format!("NAME template not used ({e}); date-time name used"))),
            };
        }
    }
    (clip_base(style.min(NAME_STYLES - 1), app, tm, millis), None)
}
pub const PAUSE_FILE: &str = "pause-key.txt";
/// Saved pause shortcut in the hotkey control's packed form. Alt+P when missing or invalid.
pub fn pause_key(dir: &std::path::Path) -> u32 {
    std::fs::read_to_string(dir.join(PAUSE_FILE))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(crate::shortcuts::normalize)
        .filter(|p| crate::shortcuts::validate(*p, 0, "").is_ok())
        .unwrap_or(crate::shortcuts::DEFAULT_PAUSE)
}
pub fn set_pause_key(dir: &std::path::Path, packed: u32) -> Result<(), String> {
    crate::save_folder::atomic_text(&dir.join(PAUSE_FILE), &crate::shortcuts::normalize(packed).to_string())
}
pub fn set_name_style(dir: &std::path::Path, style: u8) -> Result<(), String> {
    let path = dir.join("name-style.txt");
    if style == 0 {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        crate::save_folder::atomic_text(&path, &style.min(STYLE_TEMPLATE).to_string())
    }
}
pub fn name_label(style: u8) -> &'static str {
    match style {
        3 => "File names: custom template",
        1 => "File names: app name, date, time",
        2 => "File names: date and time",
        _ => "File names: clip and a number",
    }
}
/// App name reduced to letters, digits, "-" and "_" so it is safe in a file name.
pub fn clean_app_name(name: &str) -> String {
    let s: String = name
        .chars()
        .filter_map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                Some(c)
            } else if c == ' ' {
                Some('-')
            } else {
                None
            }
        })
        .take(24)
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "ZimClip".into() } else { s }
}
/// File name without extension. `t` is local (year, month, day, hour, minute, second).
pub fn clip_base(style: u8, app: &str, t: (u16, u16, u16, u16, u16, u16), millis: u128) -> String {
    let (y, mo, d, h, mi, s) = t;
    match style {
        1 => format!("{}-{y:04}-{mo:02}-{d:02}-{h:02}{mi:02}{s:02}", clean_app_name(app)),
        2 => format!("{y:04}-{mo:02}-{d:02}-{h:02}{mi:02}{s:02}"),
        _ => format!("clip-{millis}"),
    }
}
/// A base name whose .mp4 and .partial.mp4 do not exist yet; adds -2, -3... on a clash.
pub fn unique_base(dir: &std::path::Path, base: &str) -> String {
    let taken = |b: &str| dir.join(format!("{b}.mp4")).exists() || dir.join(format!("{b}.partial.mp4")).exists();
    if !taken(base) {
        return base.to_string();
    }
    (2..10000)
        .map(|n| format!("{base}-{n}"))
        .find(|b| !taken(b))
        .unwrap_or_else(|| format!("{base}-{}", std::process::id()))
}
#[cfg(test)]
mod template_tests {
    use super::*;
    const T: (u16, u16, u16, u16, u16, u16) = (2026, 11, 27, 9, 5, 7);
    #[test]
    fn renders_and_rejects_in_plain_words() {
        assert_eq!(render_template("{app}-{date}-{time}", "My Clips", T).unwrap(), "My-Clips-2026-11-27-090507");
        assert_eq!(render_template("{date}_{time}", "X", T).unwrap(), "2026-11-27_090507");
        assert_eq!(render_template("holiday-clip", "X", T).unwrap(), "holiday-clip");
        for bad in ["a/b", "a\\b", "a:b", "a.b", "a*b", "a\"b", "..", "é"] {
            assert!(render_template(bad, "X", T).unwrap_err().contains("not allowed"), "{bad}");
        }
        assert!(render_template("a b", "X", T).unwrap_err().contains("Spaces"));
        assert!(render_template("{game}", "X", T).unwrap_err().contains("Unknown token {game}"));
        assert!(render_template("{date", "X", T).unwrap_err().contains("closing"));
        assert!(render_template("a}", "X", T).unwrap_err().contains("matching"));
        assert!(render_template("   ", "X", T).is_err());
        assert!(render_template("CON", "X", T).unwrap_err().contains("reserved"));
        assert!(render_template("nul", "X", T).is_err());
        assert!(render_template("{app}", "CON", T).is_err());
        assert!(render_template("CON1", "X", T).is_ok());
        assert!(render_template(&"a".repeat(60), "X", T).is_ok());
        assert!(render_template(&"a".repeat(61), "X", T).unwrap_err().contains("61-character"));
    }
    #[test]
    fn longest_app_name_is_checked_up_front() {
        // 60 - 24 = 36 literal characters fit next to a 24-character app name.
        assert!(check_template(&format!("{{app}}{}", "a".repeat(36)), "Z").is_ok());
        assert!(check_template(&format!("{{app}}{}", "a".repeat(37)), "Z").is_err());
        assert!(check_template("{date}-{time}", "Z").is_ok());
    }
    #[test]
    fn save_switch_rollback_and_reload() {
        let d = std::env::temp_dir().join(format!("zimclip-tpl-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        assert!(set_template(&d, "bad/name", "Z").is_err());
        assert_eq!(name_style(&d), 0);
        assert!(!d.join("name-template.txt").exists());
        set_template(&d, "{app}-{date}", "Z").unwrap();
        assert_eq!(name_style(&d), STYLE_TEMPLATE);
        assert_eq!(template(&d).as_deref(), Some("{app}-{date}"));
        assert_eq!(save_base(&d, "Z", T, 1).0, "Z-2026-11-27");
        // Switching to a preset keeps the custom template and does not edit it.
        set_name_style(&d, 2).unwrap();
        assert_eq!(name_style(&d), 2);
        assert_eq!(template(&d).as_deref(), Some("{app}-{date}"));
        assert_eq!(save_base(&d, "Z", T, 1).0, "2026-11-27-090507");
        // A write failure of the style file rolls the template back.
        set_template(&d, "first", "Z").unwrap();
        set_name_style(&d, 1).unwrap();
        std::fs::create_dir(d.join("name-style.tmp")).unwrap(); // makes the style write fail
        assert!(set_template(&d, "second", "Z").is_err());
        assert_eq!(template(&d).as_deref(), Some("first"));
        assert_eq!(name_style(&d), 1);
        std::fs::remove_dir(d.join("name-style.tmp")).unwrap();
        // A hand-edited invalid template falls back to the old default, never to a bad name.
        std::fs::write(d.join("name-template.txt"), "../x").unwrap();
        std::fs::write(d.join("name-style.txt"), "3").unwrap();
        assert_eq!(name_style(&d), 0);
        std::fs::remove_dir_all(&d).ok();
    }
    #[test]
    fn pause_key_storage() {
        let d = std::env::temp_dir().join(format!("zimclip-pause-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(pause_key(&d), crate::shortcuts::DEFAULT_PAUSE);
        set_pause_key(&d, 0x646).unwrap();
        assert_eq!(pause_key(&d), 0x646);
        std::fs::write(d.join(PAUSE_FILE), "junk").unwrap();
        assert_eq!(pause_key(&d), crate::shortcuts::DEFAULT_PAUSE);
        std::fs::write(d.join(PAUSE_FILE), "1103").unwrap(); // Alt+O, reserved
        assert_eq!(pause_key(&d), crate::shortcuts::DEFAULT_PAUSE);
        std::fs::remove_dir_all(&d).ok();
    }
}
#[cfg(test)]
mod name_tests {
    use super::*;
    #[test]
    fn names_are_safe_and_unique() {
        let t = (2026, 11, 27, 9, 5, 7);
        assert_eq!(clip_base(0, "X", t, 123), "clip-123");
        assert_eq!(clip_base(1, "My Clips!", t, 1), "My-Clips-2026-11-27-090507");
        assert_eq!(clip_base(1, "***", t, 1), "ZimClip-2026-11-27-090507");
        assert_eq!(clip_base(2, "X", t, 1), "2026-11-27-090507");
        assert_eq!(clean_app_name(&"a".repeat(40)).len(), 24);
        let d = std::env::temp_dir().join(format!("zimclip-name-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.mp4"), b"x").unwrap();
        std::fs::write(d.join("a-2.partial.mp4"), b"x").unwrap();
        assert_eq!(unique_base(&d, "b"), "b");
        assert_eq!(unique_base(&d, "a"), "a-3");
        assert_eq!(name_style(&d), 0);
        set_name_style(&d, 2).unwrap();
        assert_eq!(name_style(&d), 2);
        set_name_style(&d, 0).unwrap();
        assert_eq!(name_style(&d), 0);
        std::fs::remove_dir_all(&d).ok();
    }
}
