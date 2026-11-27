//! Control ids added to the main window and Settings view in 72+.
//! One id namespace is shared by: the main window (Home/Replay cards 100-104, 120-125, buttons
//! 1-90), the Settings view (Appearance choice swatches 300-318, controls re-parented into it,
//! owner-draw and WM_COMMAND forwarded up to the main window), library chips 400-421, editor and
//! welcome controls (430-520), popup menu commands 7100-7199. Pick new ids outside all of these. Keep them here and use the
//! names everywhere; the test below fails if one of these numbers is typed as a literal
//! elsewhere, or if one lands inside an id block another view already owns.
pub const PAUSE_KEY: usize = 8800;
pub const APPLY_PAUSE: usize = 8801;
pub const TEMPLATE_EDIT: usize = 8802;
pub const APPLY_TEMPLATE: usize = 8803;
/// Not a control: the message posted while the template text changes.
pub const TEMPLATE_PREVIEW: usize = 8804;
/// Sidebar Home sub-page switch (Capture / Sources). Inside the free 161-205 block, checked
/// against the RESERVED list below and by the literal-scan tests.
pub const HOME_SUBPAGE: usize = 161;
/// Replay length slider (5..=300 s, steps of 5) and its four quick-length buttons.
pub const REPLAY_SLIDER: usize = 8805;
pub const QUICK_LEN: [usize; 4] = [8806, 8807, 8808, 8809];
/// Seconds for each quick button, in the same order as `QUICK_LEN`.
/// Editable tagline under the app name (Appearance page).
pub const TAGLINE_EDIT: usize = 8810;
pub const QUICK_SECS: [u64; 4] = [15, 30, 60, 120];
pub const ALL: [usize; 12] = [TAGLINE_EDIT, PAUSE_KEY, APPLY_PAUSE, TEMPLATE_EDIT, APPLY_TEMPLATE, TEMPLATE_PREVIEW, HOME_SUBPAGE, REPLAY_SLIDER, QUICK_LEN[0], QUICK_LEN[1], QUICK_LEN[2], QUICK_LEN[3]];
/// Id blocks owned by other code (Replay cards, Home cards, Appearance choice cards,
/// library chips, editor and dialog controls, menu commands). New ids must stay outside.
#[cfg(test)]
const RESERVED: [(usize, usize); 10] = [
    (0, 160),
    (206, 215),
    (220, 224),
    (300, 318),
    (400, 421),
    (430, 520),
    (1001, 1001),
    (2000, 2999),
    (7100, 7199),
    (9000, 9999),
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_distinct_and_outside_reserved_blocks() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(!ALL[i + 1..].contains(a), "duplicate id {a}");
            for (lo, hi) in RESERVED {
                assert!(!(lo..=hi).contains(a), "id {a} is inside reserved block {lo}..={hi}");
            }
        }
    }
    /// No `A..B` / `A..=B` literal range anywhere in the sources may contain one of our ids.
    #[test]
    fn no_source_range_contains_our_ids() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            // Only files that create or route controls; others use unrelated number ranges.
            let ui_files = ["main.rs", "ui.rs", "display.rs", "settings_view.rs", "replay_design.rs", "library_view.rs", "polished.rs", "nav.rs", "welcome.rs", "editor.rs", "custom_dialog.rs", "audio_devices.rs"];
            if !ui_files.contains(&name.as_str()) {
                continue;
            }
            for line in std::fs::read_to_string(&p).unwrap().lines() {
                // Only lines that test or loop over control or command ids.
                let idish = ["CtlID", "DlgCtrlID", "&id)", "&id,", "id in ", "wParam", "&cmd", "GetDlgItem"];
                if !idish.iter().any(|t| line.contains(t)) {
                    continue;
                }
                let b = line.as_bytes();
                let mut i = 0;
                while i + 1 < b.len() {
                    if b[i] == b'.' && b[i + 1] == b'.' {
                        let mut l = i;
                        while l > 0 && b[l - 1].is_ascii_digit() {
                            l -= 1;
                        }
                        let mut r = i + 2;
                        if r < b.len() && b[r] == b'=' {
                            r += 1;
                        }
                        let start = r;
                        while r < b.len() && b[r].is_ascii_digit() {
                            r += 1;
                        }
                        if l < i && r > start {
                            let lo: usize = line[l..i].parse().unwrap_or(0);
                            let hi: usize = line[start..r].parse().unwrap_or(0);
                            for id in ALL {
                                assert!(!(lo..=hi).contains(&id), "range {lo}..{hi} in {} contains id {id}", p.display());
                            }
                        }
                        i = r.max(i + 2);
                    } else {
                        i += 1;
                    }
                }
            }
        }
    }
    #[test]
    fn no_literal_copies_of_these_ids_elsewhere() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().map_or(true, |x| x != "rs") || p.file_name().unwrap() == "ctl_ids.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for id in ALL {
                let lit = id.to_string();
                let b = text.as_bytes();
                let mut from = 0;
                while let Some(i) = text[from..].find(&lit) {
                    let s = from + i;
                    let before = s == 0 || !b[s - 1].is_ascii_alphanumeric() && b[s - 1] != b'_' && b[s - 1] != b'.';
                    let after = s + lit.len() >= b.len() || !b[s + lit.len()].is_ascii_alphanumeric();
                    assert!(!(before && after), "literal {lit} in {}: use ctl_ids", p.display());
                    from = s + lit.len();
                }
            }
        }
    }
}
#[cfg(test)]
mod lookup_tests {
    /// The Home sub-page switch is looked up by its registry name everywhere. A stale literal
    /// (an earlier draft used 115) would make GetDlgItem return NULL and silently skip layout.
    #[test]
    fn home_subpage_is_never_looked_up_by_a_stale_literal() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().map_or(true, |x| x != "rs") || p.file_name().unwrap() == "ctl_ids.rs" { continue; }
            let text = std::fs::read_to_string(&p).unwrap();
            assert!(!text.contains("GetDlgItem(h, 115)") && !text.contains("GetDlgItem(hwnd, 115)") && !text.contains("GetDlgItem(parent, 115)"), "stale 115 lookup in {}", p.display());
        }
        let layout = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/replay_design.rs")).unwrap();
        assert!(layout.matches("ctl_ids::HOME_SUBPAGE").count() >= 4, "layout must resolve the switch through ctl_ids");
    }
}
