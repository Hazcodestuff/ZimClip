//! Custom colour theme (background, panels, text) and named presets. Pure logic and files only.
//! Colours are GDI 0x00BBGGRR everywhere in memory and RRGGBB hex in files.
use std::{path::Path, sync::Mutex};

pub const THEME_INDEX: usize = 6;
pub const MAX_PRESETS: usize = 64;
pub const MAX_NAME: usize = 24;
const MAX_FILE: u64 = 16 * 1024;
pub const MIN_CONTRAST: f64 = 4.5;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Custom {
    pub bg: u32,
    pub panel: u32,
    pub text: u32,
}
pub const DEFAULT: Custom = Custom { bg: 0x120f0d, panel: 0x1b1714, text: 0xf4f1ee };

#[derive(Clone, PartialEq, Debug)]
pub struct Preset {
    pub name: String,
    pub colors: Custom,
    pub accent: usize,
}

static CURRENT: Mutex<Custom> = Mutex::new(DEFAULT);
pub fn current() -> Custom {
    *CURRENT.lock().unwrap()
}
pub fn set_current(c: Custom) {
    *CURRENT.lock().unwrap() = c;
}

fn mix(a: u32, b: u32, p: u32) -> u32 {
    let mut c = 0;
    for s in [0, 8, 16] {
        c |= ((a >> s & 255) * (100 - p) + (b >> s & 255) * p) / 100 << s;
    }
    c
}
fn lum(c: u32) -> f64 {
    let ch = |x: u32| {
        let v = x as f64 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c & 255) + 0.7152 * ch(c >> 8 & 255) + 0.0722 * ch(c >> 16 & 255)
}
pub fn contrast(a: u32, b: u32) -> f64 {
    let (a, b) = (lum(a), lum(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// [bg, panel, border, text, muted]. Border and muted are derived; the picked colours are never changed.
pub fn palette(c: Custom) -> [u32; 5] {
    let border = mix(c.panel, c.text, 22);
    let muted = mix(c.text, c.panel, 30);
    [c.bg, c.panel, border, c.text, muted]
}

/// Plain-words warning when the text colour is hard to read. Never changes a colour.
pub fn warning(c: Custom) -> Option<String> {
    let on_bg = contrast(c.text, c.bg);
    let on_panel = contrast(c.text, c.panel);
    let worst = on_bg.min(on_panel);
    (worst < MIN_CONTRAST).then(|| {
        let where_ = if on_bg <= on_panel { "background" } else { "panels" };
        format!("Text is hard to read on the {where_} ({worst:.1}:1; 4.5:1 is comfortable). You can reset to a built-in theme below.")
    })
}

fn hex(c: u32) -> String {
    format!("{:02x}{:02x}{:02x}", c & 255, c >> 8 & 255, c >> 16 & 255)
}
fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some((v & 255) << 16 | (v >> 8 & 255) << 8 | v >> 16 & 255)
}

pub fn encode_custom(c: Custom) -> String {
    format!("bg={}\npanel={}\ntext={}\n", hex(c.bg), hex(c.panel), hex(c.text))
}
/// Missing or damaged values fall back to the default for that field.
pub fn parse_custom(s: &str) -> Custom {
    let mut c = DEFAULT;
    for line in s.lines().take(16) {
        if let Some((k, v)) = line.split_once('=') {
            if let Some(x) = parse_hex(v) {
                match k.trim() {
                    "bg" => c.bg = x,
                    "panel" => c.panel = x,
                    "text" => c.text = x,
                    _ => {}
                }
            }
        }
    }
    c
}

pub fn clean_name(s: &str) -> Result<String, String> {
    let n: String = s.chars().filter(|c| !c.is_control() && *c != '\t').collect();
    let n = n.trim().to_string();
    if n.is_empty() {
        return Err("Type a name for this preset.".into());
    }
    if n.chars().count() > MAX_NAME {
        return Err(format!("Preset names are up to {MAX_NAME} characters."));
    }
    Ok(n)
}

/// One preset per line: name TAB bg TAB panel TAB text TAB accent.
pub fn encode_presets(p: &[Preset]) -> String {
    p.iter()
        .map(|x| format!("{}\t{}\t{}\t{}\t{}\n", x.name, hex(x.colors.bg), hex(x.colors.panel), hex(x.colors.text), x.accent))
        .collect()
}
pub fn parse_presets(s: &str) -> Vec<Preset> {
    let mut out: Vec<Preset> = Vec::new();
    for line in s.lines().take(MAX_PRESETS * 2) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 5 {
            continue;
        }
        let (Ok(name), Some(bg), Some(panel), Some(text), Ok(accent)) =
            (clean_name(f[0]), parse_hex(f[1]), parse_hex(f[2]), parse_hex(f[3]), f[4].trim().parse::<usize>())
        else {
            continue;
        };
        if accent >= 8 || out.iter().any(|p| p.name == name) || out.len() >= MAX_PRESETS {
            continue;
        }
        out.push(Preset { name, colors: Custom { bg, panel, text }, accent });
    }
    out
}
/// Same name replaces that preset; a new name is appended up to the file limit.
pub fn upsert(list: &mut Vec<Preset>, p: Preset) -> Result<(), String> {
    if let Some(e) = list.iter_mut().find(|x| x.name == p.name) {
        *e = p;
        return Ok(());
    }
    if list.len() >= MAX_PRESETS {
        return Err(format!("You have {MAX_PRESETS} presets already. Delete one first."));
    }
    list.push(p);
    Ok(())
}

fn read_small(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) if m.len() <= MAX_FILE => std::fs::read_to_string(path).unwrap_or_default(),
        _ => String::new(),
    }
}
fn write_atomic(path: &Path, body: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}
pub fn load(root: &Path) {
    set_current(parse_custom(&read_small(&root.join("custom-theme.ini"))));
}
pub fn load_presets(root: &Path) -> Vec<Preset> {
    parse_presets(&read_small(&root.join("theme-presets.ini")))
}
pub fn save_custom(root: &Path, c: Custom) -> Result<(), String> {
    write_atomic(&root.join("custom-theme.ini"), &encode_custom(c))
}
pub fn save_presets(root: &Path, p: &[Preset]) -> Result<(), String> {
    write_atomic(&root.join("theme-presets.ini"), &encode_presets(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_round_trip_and_bgr() {
        let c = Custom { bg: 0x0000ff, panel: 0x00ff00, text: 0xff0000 }; // BGR: red, green, blue
        let s = encode_custom(c);
        assert!(s.contains("bg=ff0000") && s.contains("panel=00ff00") && s.contains("text=0000ff"));
        assert_eq!(parse_custom(&s), c);
    }
    #[test]
    fn damaged_values_fall_back() {
        let c = parse_custom("bg=zzzzzz\npanel=123\ntext=ffffff\n\u{0}garbage");
        assert_eq!(c.bg, DEFAULT.bg);
        assert_eq!(c.panel, DEFAULT.panel);
        assert_eq!(c.text, 0xffffff);
    }
    #[test]
    fn warning_only_when_unreadable_and_never_edits() {
        assert!(warning(DEFAULT).is_none());
        let bad = Custom { bg: 0x808080, panel: 0x7f7f7f, text: 0x858585 };
        let w = warning(bad).unwrap();
        assert!(w.contains("hard to read") && w.contains("reset to a built-in"));
        assert_eq!(palette(bad)[0], bad.bg);
        assert_eq!(palette(bad)[1], bad.panel);
        assert_eq!(palette(bad)[3], bad.text);
    }
    #[test]
    fn contrast_known_values() {
        assert!((contrast(0, 0xffffff) - 21.0).abs() < 0.01);
        assert!((contrast(0x777777, 0x777777) - 1.0).abs() < 1e-9);
    }
    #[test]
    fn presets_round_trip_with_accent_and_reject_bad_lines() {
        let a = Preset { name: "Night owl".into(), colors: DEFAULT, accent: 3 };
        let b = Preset { name: "Paper".into(), colors: Custom { bg: 0xffffff, panel: 0xeeeeee, text: 0 }, accent: 7 };
        let text = encode_presets(&[a.clone(), b.clone()]) + "bad line\nx\t1\t2\t3\t4\nDup\t000000\t000000\t000000\t9\n";
        let got = parse_presets(&text);
        assert_eq!(got, vec![a, b]);
    }
    #[test]
    fn names_are_validated_and_upsert_replaces() {
        assert!(clean_name("   ").is_err());
        assert!(clean_name(&"x".repeat(25)).is_err());
        assert_eq!(clean_name(" a\tb\n ").unwrap(), "ab");
        let mut l = vec![];
        upsert(&mut l, Preset { name: "A".into(), colors: DEFAULT, accent: 0 }).unwrap();
        upsert(&mut l, Preset { name: "A".into(), colors: DEFAULT, accent: 5 }).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].accent, 5);
        for i in 0..MAX_PRESETS {
            let _ = upsert(&mut l, Preset { name: format!("p{i}"), colors: DEFAULT, accent: 0 });
        }
        assert_eq!(l.len(), MAX_PRESETS);
        assert!(upsert(&mut l, Preset { name: "overflow".into(), colors: DEFAULT, accent: 0 }).is_err());
    }
    #[test]
    fn files_are_atomic_and_size_bounded() {
        let d = std::env::temp_dir().join(format!("zim-ct-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let c = Custom { bg: 0x010203, panel: 0x040506, text: 0xfafbfc };
        save_custom(&d, c).unwrap();
        load(&d);
        assert_eq!(current(), c);
        assert!(!d.join("custom-theme.tmp").exists());
        std::fs::write(d.join("theme-presets.ini"), "x".repeat(20_000)).unwrap();
        assert!(load_presets(&d).is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
