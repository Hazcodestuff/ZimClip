/// RegisterHotKey modifier bits (same values as MOD_ALT, MOD_CONTROL, MOD_SHIFT).
pub const MOD_ALT: u32 = 1;
pub const MOD_CTRL: u32 = 2;
pub const MOD_SHIFT: u32 = 4;
/// Default pause / resume shortcut (Alt+P) in the hotkey control's packed form.
pub const DEFAULT_PAUSE: u32 = 0x450;
/// Hotkey control value (key in the low byte, HOTKEYF flags above) to (modifiers, key).
pub fn split(packed: u32) -> (u32, u32) {
    let flags = (packed >> 8) & 7;
    let m = (if flags & 1 != 0 { MOD_SHIFT } else { 0 })
        | (if flags & 2 != 0 { MOD_CTRL } else { 0 })
        | (if flags & 4 != 0 { MOD_ALT } else { 0 });
    (m, packed & 255)
}
/// Keeps only the key and the three modifier flags.
pub fn normalize(packed: u32) -> u32 {
    (packed & 255) | (((packed >> 8) & 7) << 8)
}
/// Rules for a global shortcut. `other` is the other global shortcut (save or pause),
/// `other_name` how the message calls it.
pub fn validate(packed: u32, other: u32, other_name: &str) -> Result<(), String> {
    let (m, k) = split(packed);
    if k == 0 || (m == 0 && !(0x70..=0x87).contains(&k)) {
        return Err("Use F1-F24 or a modified key.".into());
    }
    if m == MOD_ALT && (k == 0x4f || k == 0x51) {
        return Err("Alt+O and Alt+Q are reserved.".into());
    }
    if split(other) == (m, k) {
        return Err(format!("That is already your {other_name} shortcut."));
    }
    Ok(())
}
pub fn label(packed: u32) -> String {
    let key = packed & 255;
    let flags = (packed >> 8) & 255;
    let name = if (0x70..=0x87).contains(&key) {
        format!("F{}", key - 0x6f)
    } else if (0x30..=0x5a).contains(&key) {
        char::from_u32(key).unwrap().to_string()
    } else {
        format!("key {key}")
    };
    format!(
        "{}{}{}{}",
        if flags & 2 != 0 { "Ctrl+" } else { "" },
        if flags & 4 != 0 { "Alt+" } else { "" },
        if flags & 1 != 0 { "Shift+" } else { "" },
        name
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conflicts_and_rules_both_directions() {
        let save = 0x447; // Alt+G
        let pause = DEFAULT_PAUSE; // Alt+P
        assert!(validate(0x450, save, "Save").is_ok());
        assert!(validate(0x447, pause, "Pause").is_ok());
        // Each chooser rejects the other's key.
        assert!(validate(0x447, save, "Save").unwrap_err().contains("already your Save"));
        assert!(validate(0x450, pause, "Pause").unwrap_err().contains("already your Pause"));
        assert!(validate(0x450, pause, "Pause").is_err());
        // Alt+P is free for Save once Pause moved away.
        assert!(validate(0x450, 0x457, "Pause").is_ok());
        assert!(validate(0x44f, pause, "Pause").unwrap_err().contains("reserved"));
        assert!(validate(0x451, pause, "Pause").unwrap_err().contains("reserved"));
        assert!(validate(0, pause, "Pause").is_err());
        assert!(validate(0x50, pause, "Pause").is_err()); // bare P
        assert!(validate(0x78, pause, "Pause").is_ok()); // bare F9
        assert_eq!(split(0x646), (MOD_ALT | MOD_CTRL, 0x46));
        assert_eq!(normalize(0xff0450), 0x450);
    }
    #[test]
    fn function_names() {
        assert_eq!(label(0x78), "F9");
        assert_eq!(label(0x447), "Alt+G");
        assert_eq!(label(0x278), "Ctrl+F9");
    }
}
