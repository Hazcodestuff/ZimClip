//! Pure helpers for the first-run setup pages. No Windows calls, so they are unit tested.

/// Common voice-chat apps shown as one-click choices. Each entry lists the process
/// names (lower case, without ".exe") that the choice adds or removes together.
/// Names are the current Windows desktop executables: Discord.exe, WhatsApp.Root.exe
/// (Microsoft Store WhatsApp), Telegram.exe, ms-teams.exe (new Teams; Microsoft Learn),
/// Zoom.exe, slack.exe, Signal.exe, Skype.exe.
pub const COMMON_APPS: [(&str, &[&str]); 8] = [
    ("Discord", &["discord"]),
    ("WhatsApp", &["whatsapp.root"]),
    ("Telegram", &["telegram"]),
    ("Teams", &["ms-teams"]),
    ("Zoom", &["zoom"]),
    ("Slack", &["slack"]),
    ("Signal", &["signal"]),
    ("Skype", &["skype"]),
];

fn key(token: &str) -> String {
    let t = token.trim().to_lowercase();
    t.strip_suffix(".exe").unwrap_or(&t).to_string()
}

fn tokens(list: &str) -> Vec<String> {
    list.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// True when any of the group's names is already in the list.
pub fn group_selected(list: &str, names: &[&str]) -> bool {
    tokens(list).iter().any(|t| names.contains(&key(t).as_str()))
}

/// Turn a group on or off. Other entries, typed by the user, stay exactly as typed.
pub fn toggle_group(list: &str, names: &[&str]) -> String {
    let mut items = tokens(list);
    if group_selected(list, names) {
        items.retain(|t| !names.contains(&key(t).as_str()));
    } else {
        for n in names {
            if !items.iter().any(|t| key(t) == *n) {
                items.push((*n).to_string());
            }
        }
    }
    items.join(", ")
}

/// Fractions (thousandths of width and height) of the blocks drawn for each layout.
/// Each block is (left, top, right, bottom).
pub fn layout_blocks(mode: usize) -> &'static [(i32, i32, i32, i32)] {
    match mode {
        0 => &[(0, 0, 280, 1000), (340, 0, 1000, 430), (340, 570, 1000, 1000)],
        1 => &[(0, 0, 1000, 340), (0, 460, 1000, 1000)],
        2 => &[
            (0, 0, 200, 1000),
            (260, 0, 1000, 260),
            (260, 370, 1000, 630),
            (260, 740, 1000, 1000),
        ],
        3 => &[(0, 0, 1000, 290), (0, 410, 1000, 680), (0, 800, 1000, 1000)],
        _ => &[(0, 0, 190, 1000), (250, 0, 1000, 440), (250, 560, 1000, 1000)],
    }
}

/// Left and right edge of card `i` in the same row.
pub fn card_span(i: i32, left: i32, total: i32, count: i32, gap: i32) -> (i32, i32) {
    let width = (total - gap * (count - 1)) / count;
    let l = left + i * (width + gap);
    (l, l + width)
}

/// Index of the card under a point in a row of `count` equal cards that starts at
/// `left`, is `total` wide and has `gap` between cards.
pub fn card_at(x: i32, left: i32, total: i32, count: i32, gap: i32) -> Option<usize> {
    if count <= 0 || x < left || x >= left + total {
        return None;
    }
    let width = (total - gap * (count - 1)) / count;
    let step = width + gap;
    let i = (x - left) / step;
    ((x - left) % step < width && i < count).then_some(i as usize)
}

/// Writes the Audio and Clips page choices when setup finishes with Done. Each value is written
/// only when the user changed it, so Skip and untouched pages never touch these files.
pub fn write_audio_and_clips(
    root: &std::path::Path,
    devices: [Option<String>; 2],
    folder: Option<&std::path::Path>,
    secs: Option<u64>,
    old_secs: u64,
    share_index: usize,
) -> Result<(), String> {
    let cur = crate::audio_devices::selected();
    let next = crate::audio_devices::Choices {
        mic: devices[0].clone().unwrap_or(cur.mic.clone()),
        system: devices[1].clone().unwrap_or(cur.system.clone()),
    };
    if next != cur {
        crate::audio_devices::save(root, next)?;
    }
    if let Some(f) = folder {
        crate::save_folder::save(root, f)?;
    }
    if let Some(v) = secs {
        if v != old_secs {
            if !(5..=300).contains(&v) {
                return Err("Clip length must be 5 to 300 seconds.".into());
            }
            crate::save_folder::atomic_text(&root.join("buffer-seconds.txt"), &v.to_string())?;
        }
    }
    let mb = (share_index > 0).then(|| crate::file_prefs::SHARE_SIZES_MB.get(share_index - 1).copied()).flatten();
    crate::file_prefs::set_share_default_mb(root, mb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_on_and_off_and_keeps_custom_entries() {
        let on = toggle_group("myapp.exe", &["discord"]);
        assert_eq!(on, "myapp.exe, discord");
        assert!(group_selected(&on, &["discord"]));
        assert_eq!(toggle_group(&on, &["discord"]), "myapp.exe");
    }

    #[test]
    fn matching_ignores_case_and_exe() {
        assert!(group_selected("Discord.exe, Zoom", &["discord"]));
        assert!(group_selected("Discord.exe, Zoom", &["zoom"]));
        assert!(!group_selected("Discord.exe", &["telegram"]));
        assert_eq!(toggle_group("Discord.EXE, Zoom", &["discord"]), "Zoom");
    }

    #[test]
    fn groups_add_all_names_once() {
        let names = ["a-app", "a-app.helper"];
        assert_eq!(toggle_group("", &names), "a-app, a-app.helper");
        assert_eq!(toggle_group("a-app", &names), "");
        assert_eq!(toggle_group("zoom, a-app", &names), "zoom");
    }

    #[test]
    fn every_common_app_is_a_valid_saved_name() {
        for (label, names) in COMMON_APPS {
            let list = toggle_group("", names);
            let saved = crate::comms::normalize(&list).unwrap_or_else(|e| panic!("{label}: {e}"));
            assert_eq!(saved.len(), names.len(), "{label}");
        }
        let all: Vec<&str> = COMMON_APPS.iter().flat_map(|(_, n)| n.iter().copied()).collect();
        let list = all.join(", ");
        assert!(crate::comms::normalize(&list).unwrap().len() <= 24);
    }

    #[test]
    fn layout_blocks_stay_inside_the_card() {
        for mode in 0..5 {
            for (l, t, r, b) in layout_blocks(mode) {
                assert!(*l >= 0 && *t >= 0 && *r <= 1000 && *b <= 1000 && l < r && t < b);
            }
        }
    }

    #[test]
    fn card_hits_match_the_gaps() {
        // 4 cards in 688px with 10px gaps: width 164.
        assert_eq!(card_at(28, 28, 688, 4, 10), Some(0));
        assert_eq!(card_at(28 + 163, 28, 688, 4, 10), Some(0));
        assert_eq!(card_at(28 + 168, 28, 688, 4, 10), None);
        assert_eq!(card_at(28 + 174, 28, 688, 4, 10), Some(1));
        assert_eq!(card_at(28 + 680, 28, 688, 4, 10), Some(3));
        assert_eq!(card_at(27, 28, 688, 4, 10), None);
        assert_eq!(card_at(716, 28, 688, 4, 10), None);
    }

    #[test]
    fn done_writes_only_changed_audio_and_clips_values() {
        let dir = std::env::temp_dir().join(format!("zc-wiz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Untouched pages write nothing (Skip never reaches this function at all).
        write_audio_and_clips(&dir, [None, None], None, None, 30, 0).unwrap();
        assert!(!dir.join("audio-devices.txt").exists());
        assert!(!dir.join("buffer-seconds.txt").exists());
        assert!(!dir.join("share-default.txt").exists());
        // Same length as before: not rewritten. Changed values land in their files.
        write_audio_and_clips(&dir, [None, None], None, Some(30), 30, 0).unwrap();
        assert!(!dir.join("buffer-seconds.txt").exists());
        let folder = dir.join("my clips");
        write_audio_and_clips(&dir, [Some("mic-id".into()), Some("out-id".into())], Some(&folder), Some(90), 30, 4).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("audio-devices.txt")).unwrap(), "mic-id\nout-id\n");
        assert_eq!(std::fs::read_to_string(dir.join("buffer-seconds.txt")).unwrap(), "90");
        assert_eq!(std::fs::read_to_string(dir.join("share-default.txt")).unwrap(), "25");
        assert_eq!(std::fs::read_to_string(dir.join("video-folder.txt")).unwrap(), folder.to_string_lossy());
        assert!(folder.is_dir());
        assert!(write_audio_and_clips(&dir, [None, None], None, Some(301), 90, 0).is_err());
        // Choosing None removes the share default.
        write_audio_and_clips(&dir, [None, None], None, None, 90, 0).unwrap();
        assert!(!dir.join("share-default.txt").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
