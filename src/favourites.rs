//! Favourite clips. Stored in their own sidecar file in the clips folder, one hex-encoded file
//! name per line, so older builds (which never read it) and the tag file are untouched. A clip
//! that is not listed is simply not a favourite; a missing or unreadable file means none.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

const FILE: &str = ".zimclip-favourites.txt";

fn key(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().bytes().map(|b| format!("{b:02x}")).collect()
}

fn valid_line(l: &str) -> bool {
    l.len() % 2 == 0 && l.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Reads the list for a write. A file that cannot be read as text or holds non-list lines is
/// damaged: it is copied to `.zimclip-favourites.txt.damaged` (the first copy is never replaced)
/// and the list is treated as empty. If that copy cannot be made the write is refused, so a
/// damaged list is never overwritten without a preserved copy.
fn load_for_write(dir: &Path) -> Result<BTreeSet<String>, String> {
    let path = dir.join(FILE);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => {
            keep_damaged(dir)?;
            return Err(format!("Favourites list unreadable ({e}); not changed."));
        }
    };
    match parse(&bytes) {
        Some(set) => Ok(set),
        None => {
            keep_damaged(dir)?;
            Ok(BTreeSet::new())
        }
    }
}

/// Reads the list for display and filtering: damaged or unreadable means no favourites shown.
fn load(dir: &Path) -> BTreeSet<String> {
    std::fs::read(dir.join(FILE)).ok().and_then(|b| parse(&b)).unwrap_or_default()
}

fn parse(bytes: &[u8]) -> Option<BTreeSet<String>> {
    let t = std::str::from_utf8(bytes).ok()?;
    if !t.lines().map(str::trim).all(|l| l.is_empty() || valid_line(l)) {
        return None;
    }
    Some(t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

fn keep_damaged(dir: &Path) -> Result<(), String> {
    let keep = dir.join(".zimclip-favourites.txt.damaged");
    if keep.exists() {
        return Ok(());
    }
    // Copy to a temporary name and rename, so a partial copy can never stand as the preserved one.
    let tmp = dir.join(".zimclip-favourites.txt.damaged.tmp");
    let fail = |e: std::io::Error| format!("Could not keep a copy of the damaged favourites list: {e}");
    let result = std::fs::copy(dir.join(FILE), &tmp).and_then(|_| std::fs::rename(&tmp, &keep));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(fail)
}

fn store(dir: &Path, set: &BTreeSet<String>) -> Result<(), String> {
    let a = dir.join(".zimclip-favourites.txt.tmp");
    let text: String = set.iter().map(|k| format!("{k}\n")).collect();
    std::fs::write(&a, text).map_err(|e| e.to_string())?;
    // std's rename replaces an existing file on every platform we build for.
    std::fs::rename(&a, dir.join(FILE)).map_err(|e| e.to_string())
}

pub fn is_favourite(p: &Path) -> bool {
    p.parent().is_some_and(|d| load(d).contains(&key(p)))
}

/// Flips the favourite flag and returns the new state.
pub fn toggle(p: &Path) -> Result<bool, String> {
    let dir = p.parent().ok_or("Missing clips folder")?;
    let mut set = load_for_write(dir)?;
    let k = key(p);
    let on = !set.remove(&k);
    if on {
        set.insert(k);
    }
    store(dir, &set)?;
    Ok(on)
}

/// Keeps the flag when a clip is renamed.
pub fn renamed(old: &Path, new: &Path) -> Result<(), String> {
    let dir = old.parent().ok_or("Missing clips folder")?;
    let mut set = load_for_write(dir)?;
    if set.remove(&key(old)) {
        set.insert(key(new));
        store(dir, &set)?;
    }
    Ok(())
}

/// Moving a clip to Trash keeps its flag in the list (like tags), so moving the file back under
/// its original name keeps it a favourite. A marker beside the trashed file records the flag.
/// There is no in-app Restore feature, so nothing reads the marker.
pub fn trash_metadata(old: &Path, new: &Path) -> Result<(), String> {
    if is_favourite(old) {
        std::fs::write(new.with_extension("mp4.fav.txt"), "favourite").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Keeps only favourites.
pub fn only(files: Vec<PathBuf>, dir: &Path) -> Vec<PathBuf> {
    let set = load(dir);
    files.into_iter().filter(|p| set.contains(&key(p))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zim-fav-{n}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    #[test]
    fn toggle_persists_and_unflags() {
        let d = dir("toggle");
        let a = d.join("a.mp4");
        assert!(!is_favourite(&a));
        assert!(toggle(&a).unwrap());
        assert!(is_favourite(&a));
        assert!(!toggle(&a).unwrap());
        assert!(!is_favourite(&a));
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn missing_or_damaged_file_means_no_favourites_and_tags_are_untouched() {
        let d = dir("compat");
        let a = d.join("odd name \u{e9}.mp4");
        std::fs::write(d.join(FILE), [0xff, 0xfe, b'\n', b'z']).unwrap();
        assert!(!is_favourite(&a));
        std::fs::write(&a, b"v").unwrap();
        crate::catalog::set_tags(&a, "Win").unwrap();
        assert!(toggle(&a).unwrap());
        assert!(is_favourite(&a));
        assert_eq!(crate::catalog::tags(&d)["odd name \u{e9}.mp4"], "Win");
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn a_failed_backup_refuses_the_write_and_keeps_the_original() {
        let d = dir("nobackup");
        let a = d.join("a.mp4");
        std::fs::write(d.join(FILE), [0xffu8, 0xfe]).unwrap();
        // The list cannot be read as a file (it is a directory), so no backup copy can be made.
        std::fs::remove_file(d.join(FILE)).unwrap();
        std::fs::create_dir(d.join(FILE)).unwrap();
        assert!(toggle(&a).is_err());
        assert!(d.join(FILE).is_dir());
        assert!(!d.join(".zimclip-favourites.txt.damaged").exists() && !d.join(".zimclip-favourites.txt.damaged.tmp").exists());
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn rename_keeps_the_flag() {
        let d = dir("rename");
        let (a, b) = (d.join("a.mp4"), d.join("b.mp4"));
        toggle(&a).unwrap();
        renamed(&a, &b).unwrap();
        assert!(!is_favourite(&a) && is_favourite(&b));
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn trash_keeps_flag_bytes_tags_and_a_marker() {
        let d = dir("trash");
        let a = d.join("keep.mp4");
        std::fs::write(&a, b"unchanged-bytes").unwrap();
        crate::catalog::set_tags(&a, "Friends").unwrap();
        toggle(&a).unwrap();
        let moved = crate::library::trash(&a, &d).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read_to_string(moved.with_extension("mp4.fav.txt")).unwrap(), "favourite");
        // Moving the file back under its original name (by hand) is still a favourite.
        std::fs::rename(&moved, &a).unwrap();
        assert!(is_favourite(&a));
        assert_eq!(std::fs::read(&a).unwrap(), b"unchanged-bytes");
        assert_eq!(crate::catalog::tags(&d)["keep.mp4"], "Friends");
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn a_damaged_list_is_preserved_before_the_next_write() {
        let d = dir("damaged");
        let (a, b) = (d.join("a.mp4"), d.join("b.mp4"));
        let junk = [0xffu8, 0xfe, b'\n', b'z'];
        std::fs::write(d.join(FILE), junk).unwrap();
        assert!(toggle(&a).unwrap());
        assert_eq!(std::fs::read(d.join(".zimclip-favourites.txt.damaged")).unwrap(), junk);
        // A second write must not replace the first preserved copy.
        std::fs::write(d.join(FILE), b"not hex\n").unwrap();
        toggle(&b).unwrap();
        assert_eq!(std::fs::read(d.join(".zimclip-favourites.txt.damaged")).unwrap(), junk);
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn only_filters_to_favourites() {
        let d = dir("only");
        let (a, b) = (d.join("a.mp4"), d.join("b.mp4"));
        toggle(&b).unwrap();
        assert_eq!(only(vec![a, b.clone()], &d), vec![b]);
        std::fs::remove_dir_all(d).unwrap();
    }
}
