use std::{
    path::Path,
    sync::{OnceLock, RwLock},
};
fn value() -> &'static RwLock<String> {
    static NAME: OnceLock<RwLock<String>> = OnceLock::new();
    NAME.get_or_init(|| RwLock::new("ZimClip".into()))
}
pub fn name() -> String {
    value().read().unwrap().clone()
}
pub fn validate(input: &str) -> Result<String, &'static str> {
    let s = input.trim();
    if s.chars().any(char::is_control) {
        return Err("Use one line without control characters.");
    }
    if s.encode_utf16().count() > 24 {
        return Err("Use up to 24 characters.");
    }
    Ok(if s.is_empty() {
        "ZimClip".into()
    } else {
        s.into()
    })
}
pub fn load(root: &Path) {
    let s = std::fs::read_to_string(root.join("identity.txt")).unwrap_or_default();
    *value().write().unwrap() = validate(&s).unwrap_or_else(|_| "ZimClip".into());
}
pub fn save(root: &Path, input: &str) -> Result<String, String> {
    let s = validate(input).map_err(str::to_string)?;
    crate::save_folder::atomic_text(&root.join("identity.txt"), &s)?;
    *value().write().unwrap() = s.clone();
    Ok(s)
}

pub const DEFAULT_TAGLINE: &str = "Keep the moment. Leave the rest.";
pub const MAX_TAGLINE: usize = 48;
fn tagline_value() -> &'static RwLock<String> {
    static T: OnceLock<RwLock<String>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(DEFAULT_TAGLINE.into()))
}
pub fn tagline() -> String {
    tagline_value().read().unwrap().clone()
}
/// One line, at most 48 characters. Empty goes back to the default line.
pub fn validate_tagline(input: &str) -> Result<String, &'static str> {
    let s = input.trim();
    if s.chars().any(char::is_control) {
        return Err("Use one line without control characters.");
    }
    if s.encode_utf16().count() > MAX_TAGLINE {
        return Err("Use up to 48 characters.");
    }
    Ok(if s.is_empty() { DEFAULT_TAGLINE.into() } else { s.into() })
}
pub fn load_tagline(root: &Path) {
    let s = std::fs::read_to_string(root.join("tagline.txt")).unwrap_or_default();
    *tagline_value().write().unwrap() = validate_tagline(&s).unwrap_or_else(|_| DEFAULT_TAGLINE.into());
}
pub fn save_tagline(root: &Path, input: &str) -> Result<String, String> {
    let s = validate_tagline(input).map_err(str::to_string)?;
    crate::save_folder::atomic_text(&root.join("tagline.txt"), &s)?;
    *tagline_value().write().unwrap() = s.clone();
    Ok(s)
}

/// Side of the stored custom icon, in pixels. The file holds premultiplied BGRA rows.
pub const UPLOAD_SIDE: usize = 64;
pub const UPLOAD_BYTES: usize = UPLOAD_SIDE * UPLOAD_SIDE * 4;
const UPLOAD_MAGIC: &[u8; 4] = b"ZIC1";
/// Icon choice: 0 = ZimClip default, 1..=GLYPH_ICONS = built-in glyph, GLYPH_ICONS + 1 = uploaded image.
fn icon_state() -> &'static RwLock<(usize, Option<Vec<u8>>)> {
    static ICON: OnceLock<RwLock<(usize, Option<Vec<u8>>)>> = OnceLock::new();
    ICON.get_or_init(|| RwLock::new((0, None)))
}
pub fn custom_choice() -> usize {
    crate::welcome_draw::GLYPH_ICONS + 1
}
pub fn icon_choice() -> usize {
    icon_state().read().unwrap().0
}
pub fn upload_pixels() -> Option<Vec<u8>> {
    icon_state().read().unwrap().1.clone()
}
pub fn parse_icon(s: &str) -> usize {
    let s = s.trim();
    if s == "custom" {
        return custom_choice();
    }
    match s.strip_prefix("glyph:").and_then(|n| n.parse::<usize>().ok()) {
        Some(n) if (1..=crate::welcome_draw::GLYPH_ICONS).contains(&n) => n,
        _ => 0,
    }
}
pub fn icon_text(choice: usize) -> String {
    if choice == custom_choice() {
        "custom".into()
    } else if choice >= 1 && choice <= crate::welcome_draw::GLYPH_ICONS {
        format!("glyph:{choice}")
    } else {
        "default".into()
    }
}
pub fn encode_upload(px: &[u8]) -> Option<Vec<u8>> {
    (px.len() == UPLOAD_BYTES).then(|| {
        let mut v = UPLOAD_MAGIC.to_vec();
        v.extend_from_slice(px);
        v
    })
}
pub fn decode_upload(file: &[u8]) -> Option<Vec<u8>> {
    (file.len() == UPLOAD_BYTES + 4 && &file[..4] == UPLOAD_MAGIC).then(|| file[4..].to_vec())
}
/// Changes every time the icon is (re)loaded, so renderers can drop a cached icon even when
/// the choice number is the same (a new uploaded image under the same custom choice).
pub static ICON_GEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub fn load_icon(root: &Path) {
    let choice = parse_icon(&std::fs::read_to_string(root.join("icon.txt")).unwrap_or_default());
    let pixels = std::fs::read(root.join("custom-icon.bin")).ok().and_then(|f| decode_upload(&f));
    // A missing or damaged upload falls back to the default icon.
    let choice = if choice == custom_choice() && pixels.is_none() { 0 } else { choice };
    *icon_state().write().unwrap() = (choice, pixels);
    ICON_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
/// Save the icon choice. The image file is written first, then the choice, each through a
/// temp file, so a failure leaves the previous icon in place.
pub fn save_icon(root: &Path, choice: usize, pixels: Option<&[u8]>) -> Result<(), String> {
    let choice = if choice > custom_choice() { 0 } else { choice };
    if choice == custom_choice() {
        let data = pixels.and_then(encode_upload).ok_or("The chosen image is not usable.")?;
        write_atomic(&root.join("custom-icon.bin"), &data)?;
    }
    write_atomic(&root.join("icon.txt"), icon_text(choice).as_bytes())?;
    load_icon(root);
    Ok(())
}
fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

/// Largest accepted source image file, in bytes, and side, in pixels.
pub const MAX_FILE: u64 = 8 * 1024 * 1024;
pub const MAX_SIDE: u32 = 8192;
pub const MIN_SIDE: u32 = 16;
/// Centered square crop of a w x h image: (x, y, side). None when the image is too big or small.
/// Decoded-pixel budget checked from the header size before any pixel is read.
pub const MAX_PIXELS: u64 = 16_000_000;
pub fn crop_square(w: u32, h: u32) -> Option<(u32, u32, u32)> {
    if w < MIN_SIDE || h < MIN_SIDE || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return None;
    }
    let side = w.min(h);
    Some(((w - side) / 2, (h - side) / 2, side))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop_rejects_over_pixel_budget() {
        assert!(crop_square(8192, 8192).is_none());
        assert!(crop_square(4000, 4000).is_some());
        assert!(crop_square(4000, 4001).is_none());
    }
    #[test]
    fn icon_round_trip() {
        assert_eq!(parse_icon("glyph:3"), 3);
        assert_eq!(parse_icon("glyph:0"), 0);
        assert_eq!(parse_icon("glyph:99"), 0);
        assert_eq!(parse_icon("custom"), custom_choice());
        assert_eq!(parse_icon("junk"), 0);
        assert_eq!(icon_text(parse_icon("glyph:5")), "glyph:5");
        let px = vec![7u8; UPLOAD_BYTES];
        let f = encode_upload(&px).unwrap();
        assert_eq!(decode_upload(&f).unwrap(), px);
        assert!(encode_upload(&px[1..]).is_none());
        assert!(decode_upload(&f[1..]).is_none());
    }
    #[test]
    fn icon_save_and_fallback() {
        let dir = std::env::temp_dir().join(format!("zc-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(save_icon(&dir, custom_choice(), None).is_err());
        assert!(!dir.join("icon.txt").exists());
        save_icon(&dir, custom_choice(), Some(&vec![1u8; UPLOAD_BYTES])).unwrap();
        assert_eq!(icon_choice(), custom_choice());
        std::fs::write(dir.join("custom-icon.bin"), b"damaged").unwrap();
        load_icon(&dir);
        assert_eq!(icon_choice(), 0);
        save_icon(&dir, 4, None).unwrap();
        assert_eq!(icon_choice(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn crops() {
        assert_eq!(crop_square(100, 100), Some((0, 0, 100)));
        assert_eq!(crop_square(300, 100), Some((100, 0, 100)));
        assert_eq!(crop_square(100, 301), Some((0, 100, 100)));
        assert_eq!(crop_square(8, 100), None);
        assert_eq!(crop_square(9000, 100), None);
    }
    #[test]
    fn names() {
        assert_eq!(validate(" Replay ").unwrap(), "Replay");
        assert_eq!(validate("").unwrap(), "ZimClip");
        assert!(validate("bad\nname").is_err());
        assert!(validate(&"a".repeat(25)).is_err());
        assert_eq!(validate("片刻").unwrap(), "片刻");
    }
    #[test]
    fn tagline_rules() {
        assert_eq!(validate_tagline("  Clip it.  ").unwrap(), "Clip it.");
        assert_eq!(validate_tagline("   ").unwrap(), DEFAULT_TAGLINE);
        assert!(validate_tagline(&"x".repeat(49)).is_err());
        assert!(validate_tagline(&"x".repeat(48)).is_ok());
        assert!(validate_tagline("a\tb").is_err());
    }
}
