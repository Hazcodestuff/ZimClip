pub fn valid_clip_name(name: &str) -> Result<String, String> {
    let value = name.trim();
    let stem = value.strip_suffix(".mp4").unwrap_or(value);
    if stem.is_empty()
        || stem.len() > 100
        || stem.ends_with('.')
        || stem.ends_with(' ')
        || stem.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
    {
        return Err("Choose a name without path separators or Windows special characters.".into());
    }
    let reserved = stem.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&reserved.as_str())
        || ((reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && reserved.len() == 4
            && matches!(reserved.as_bytes()[3], b'1'..=b'9'))
    {
        return Err("That name is reserved by Windows. Choose another name.".into());
    }
    Ok(format!("{stem}.mp4"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_extension() {
        assert_eq!(valid_clip_name("My clip").unwrap(), "My clip.mp4");
        assert_eq!(valid_clip_name("My clip.mp4").unwrap(), "My clip.mp4");
    }
    #[test]
    fn rejects_paths_and_windows_names() {
        for name in [
            "", "../other", "a\\b", "CON", "nul.mp4", "COM1", "LPT9", "name.", "a:b", "a*b",
        ] {
            assert!(valid_clip_name(name).is_err(), "{name}");
        }
    }
    #[test]
    fn accepts_unicode() {
        assert_eq!(
            valid_clip_name("Momen terbaik").unwrap(),
            "Momen terbaik.mp4"
        );
    }
}
