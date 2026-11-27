use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
#[derive(Default, Clone)]
pub struct Filter {
    pub query: String,
    pub oldest: bool,
    pub sort: usize,
    pub tag: String,
    /// Show only favourite clips.
    pub favourites: bool,
}
pub fn filter() -> &'static Mutex<Filter> {
    static F: OnceLock<Mutex<Filter>> = OnceLock::new();
    F.get_or_init(Default::default)
}
fn key(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
fn hex(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect();
    String::from_utf8(bytes?).ok()
}
fn read_tags(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = match std::fs::read_to_string(dir.join(".zimclip-tags.tsv")) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Cannot read saved tags: {e}")),
    };
    let mut map = BTreeMap::new();
    for l in text.lines().filter(|l| !l.trim().is_empty()) {
        let (k, v) = l
            .split_once('\t')
            .ok_or("Saved tag data is damaged. Back it up before editing.")?;
        let name = unhex(k).ok_or("Saved tag name is damaged. Back it up before editing.")?;
        let value = normalize(v)?;
        map.insert(name, value);
    }
    Ok(map)
}
pub fn tags(dir: &Path) -> BTreeMap<String, String> {
    read_tags(dir).unwrap_or_default()
}
pub fn normalize(s: &str) -> Result<String, String> {
    let mut out = Vec::new();
    for t in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if t.chars().count() > 24 || t.chars().any(|c| c.is_control() || c == '#') {
            return Err("Use up to 24 characters per tag, without # or control characters.".into());
        }
        if !out
            .iter()
            .any(|x: &String| x.to_lowercase() == t.to_lowercase())
        {
            out.push(t.into());
        }
    }
    if out.len() > 8 {
        return Err("Use up to eight tags per clip.".into());
    }
    Ok(out.join(", "))
}
fn store(dir: &Path, map: &BTreeMap<String, String>) -> Result<(), String> {
    use std::io::Write;
    let text = map
        .iter()
        .map(|(k, v)| format!("{}\t{}\n", hex(k), v))
        .collect::<String>();
    let a = dir.join(".zimclip-tags.tsv.tmp");
    let b = dir.join(".zimclip-tags.tsv");
    let mut f = std::fs::File::create(&a).map_err(|e| e.to_string())?;
    f.write_all(text.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    drop(f);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let x: Vec<_> = a.as_os_str().encode_wide().chain(Some(0)).collect();
        let y: Vec<_> = b.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                x.as_ptr(),
                y.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(a, b).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn set_tags(p: &Path, s: &str) -> Result<(), String> {
    let dir = p.parent().ok_or("Missing clips folder")?;
    let mut map = read_tags(dir)?;
    let value = normalize(s)?;
    if value.is_empty() {
        map.remove(&key(p));
    } else {
        map.insert(key(p), value);
    }
    store(dir, &map)
}
pub fn renamed(old: &Path, new: &Path) -> Result<(), String> {
    let dir = old.parent().ok_or("Missing clips folder")?;
    let mut map = read_tags(dir)?;
    if let Some(t) = map.remove(&key(old)) {
        map.insert(key(new), t);
        store(dir, &map)?;
    }
    crate::favourites::renamed(old, new)?;
    Ok(())
}
pub fn matches(p: &Path, tags: &str, q: &str) -> bool {
    let name = key(p).to_lowercase();
    let tags: Vec<_> = tags.split(',').map(|s| s.trim().to_lowercase()).collect();
    q.split_whitespace().all(|word| {
        let w = word.to_lowercase();
        if let Some(tag) = w.strip_prefix('#') {
            tags.iter().any(|t| t == tag)
        } else {
            name.contains(&w) || tags.iter().any(|t| t.contains(&w))
        }
    })
}
pub fn filtered(
    mut files: Vec<PathBuf>,
    map: &BTreeMap<String, String>,
    f: &Filter,
) -> Vec<PathBuf> {
    files.retain(|p| {
        (f.tag.is_empty()
            || map
                .get(&key(p))
                .is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(&f.tag))))
            && matches(
                p,
                map.get(&key(p)).map(String::as_str).unwrap_or(""),
                &f.query,
            )
    });
    if f.sort >= 2 {
        duration_order(&mut files, f.sort == 2);
    } else if f.oldest || f.sort == 1 {
        files.reverse();
    }
    files
}
pub fn apply(files: Vec<PathBuf>, dir: &Path) -> Vec<PathBuf> {
    let f = filter().lock().unwrap().clone();
    let map = tags(dir);
    let files = if f.favourites { crate::favourites::only(files, dir) } else { files };
    filtered(files, &map, &f)
}
pub fn trash_metadata(old: &Path, new: &Path) -> Result<(), String> {
    let dir = old.parent().ok_or("Missing clips folder")?;
    if let Some(value) = read_tags(dir)?.get(&key(old)) {
        std::fs::write(new.with_extension("mp4.tags.txt"), value).map_err(|e| e.to_string())?;
    }
    crate::favourites::trash_metadata(old, new)?;
    Ok(())
}
// Tag palette is independent of per-clip assignments, so unused tags stay reusable.
const PALETTE: [u32; 8] = [
    0x9adf78, 0xe8bd7a, 0xe0a18c, 0xdb9bc2, 0x74bfe8, 0xb59de7, 0x83d6d8, 0xaaaaaa,
];
pub fn palette() -> &'static [u32] {
    &PALETTE
}
pub fn reusable(dir: &Path) -> Result<BTreeMap<String, u32>, String> {
    let mut out = BTreeMap::new();
    for value in read_tags(dir)?.values() {
        for t in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            out.entry(t.to_string()).or_insert(PALETTE[0]);
        }
    }
    let text = match std::fs::read_to_string(dir.join(".zimclip-palette.tsv")) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    for l in text.lines().filter(|s| !s.trim().is_empty()) {
        let (tag, color) = l.split_once('\t').ok_or("Tag palette is damaged")?;
        let tag = unhex(tag).ok_or("Tag palette name is damaged")?;
        if normalize(&tag)? != tag || tag.contains(',') || tag.is_empty() {
            return Err("Invalid palette tag".into());
        }
        let color = u32::from_str_radix(color, 16).map_err(|_| "Invalid tag colour")?;
        if color > 0xffffff {
            return Err("Invalid tag colour".into());
        }
        if let Some(k) = out
            .keys()
            .find(|k| k.to_lowercase() == tag.to_lowercase())
            .cloned()
        {
            out.remove(&k);
        }
        out.insert(tag, color);
    }
    Ok(out)
}
pub fn save_color(dir: &Path, tag: &str, color: u32) -> Result<(), String> {
    if normalize(tag)? != tag || tag.is_empty() || tag.contains(',') || color > 0xffffff {
        return Err("Invalid tag or colour".into());
    }
    let mut map = reusable(dir)?;
    if let Some(k) = map
        .keys()
        .find(|k| k.to_lowercase() == tag.to_lowercase())
        .cloned()
    {
        map.remove(&k);
    }
    map.insert(tag.into(), color);
    let text = map
        .iter()
        .map(|(t, c)| format!("{}\t{:06x}\n", hex(t), c))
        .collect::<String>();
    write_palette_text(dir, &text)
}
/// Durable replace of the palette file: temp file, then replace (write-through on Windows).
fn write_palette_text(dir: &Path, text: &str) -> Result<(), String> {
    let tmp = dir.join(".zimclip-palette.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    // Existing file stays intact until the complete new data has been written.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let a: Vec<_> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<_> = dir
            .join(".zimclip-palette.tsv")
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(tmp, dir.join(".zimclip-palette.tsv")).map_err(|e| e.to_string())?;
    Ok(())
}
fn cached_duration(path: &Path) -> Option<f64> {
    type Cache = BTreeMap<PathBuf, (u64, Option<std::time::SystemTime>, Option<f64>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let meta = std::fs::metadata(path).ok()?;
    let stamp = (meta.len(), meta.modified().ok());
    if let Some((n, t, v)) = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .get(path)
    {
        if (*n, *t) == stamp {
            return *v;
        }
    }
    let value = crate::media::duration(path).ok();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    if cache.len() > 10000 {
        cache.clear();
    }
    cache.insert(path.to_path_buf(), (stamp.0, stamp.1, value));
    value
}
pub fn duration_order(files: &mut [PathBuf], longest: bool) {
    let durations: BTreeMap<_, _> = files
        .iter()
        .map(|p| (p.clone(), cached_duration(p)))
        .collect();
    files.sort_by(|a, b| match (durations[a], durations[b]) {
        (Some(a), Some(b)) => {
            if longest {
                b.total_cmp(&a)
            } else {
                a.total_cmp(&b)
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duration_sort_and_unknown_last() {
        let d = std::env::temp_dir().join(format!("zim-sort-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let a = d.join("short.mp4");
        let b = d.join("long.mp4");
        let c = d.join("unknown.mp4");
        for (p, t) in [(&a, 1000u32), (&b, 9000u32)] {
            let mut bytes = Vec::new();
            bytes.extend(36u32.to_be_bytes());
            bytes.extend(b"moov");
            bytes.extend(28u32.to_be_bytes());
            bytes.extend(b"mvhd");
            bytes.extend([0; 12]);
            bytes.extend(1000u32.to_be_bytes());
            bytes.extend(t.to_be_bytes());
            std::fs::write(p, bytes).unwrap();
        }
        std::fs::write(&c, b"invalid").unwrap();
        let mut files = vec![c.clone(), a.clone(), b.clone()];
        duration_order(&mut files, true);
        assert_eq!(files, vec![b.clone(), a.clone(), c.clone()]);
        duration_order(&mut files, false);
        assert_eq!(files, vec![a, b, c]);
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn tag_filter_is_exact_and_case_insensitive() {
        let files = vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];
        let map = BTreeMap::from([
            ("a.mp4".into(), "Friends, Win".into()),
            ("b.mp4".into(), "friendship".into()),
        ]);
        assert_eq!(
            filtered(
                files,
                &map,
                &Filter {
                    tag: "friends".into(),
                    ..Default::default()
                }
            ),
            vec![PathBuf::from("a.mp4")]
        );
    }
    #[test]
    fn palette_roundtrip_preserves_assignments() {
        let d = std::env::temp_dir().join(format!("zim-palette-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("clip.mp4");
        set_tags(&p, "Friends").unwrap();
        save_color(&d, "Friends", 0x123456).unwrap();
        save_color(&d, "Unused", 0xabcdef).unwrap();
        assert_eq!(reusable(&d).unwrap()["Friends"], 0x123456);
        assert!(reusable(&d).unwrap().contains_key("Unused"));
        assert_eq!(tags(&d)["clip.mp4"], "Friends");
        std::fs::write(d.join(".zimclip-palette.tsv"), "bad").unwrap();
        assert!(save_color(&d, "Other", 1).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn tags_validation() {
        assert_eq!(normalize(" Win, win, Friends ").unwrap(), "Win, Friends");
        assert!(normalize("bad\ttag").is_err());
        assert!(normalize("#bad").is_err());
    }
    #[test]
    fn query_name_and_tag() {
        let p = Path::new("my-win.mp4");
        assert!(matches(p, "Friends, funny", "win #friends"));
        assert!(!matches(p, "friendship", "#friends"));
        assert!(!matches(p, "Friends", "lose"));
    }
    #[test]
    fn unicode_key() {
        let s = "冬-momen.mp4";
        assert_eq!(unhex(&hex(s)).unwrap(), s);
        assert!(unhex("zz").is_none());
    }
    #[test]
    fn saved_tags_and_rename() {
        let d = std::env::temp_dir().join(format!("zim-tags-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let a = d.join("a.mp4");
        let b = d.join("b.mp4");
        set_tags(&a, "fun, Friends").unwrap();
        renamed(&a, &b).unwrap();
        assert_eq!(tags(&d).get("b.mp4").unwrap(), "fun, Friends");
        set_tags(&b, "").unwrap();
        assert!(tags(&d).is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn order_and_query() {
        let files = vec![PathBuf::from("new.mp4"), PathBuf::from("old.mp4")];
        let map = BTreeMap::new();
        assert_eq!(
            filtered(
                files.clone(),
                &map,
                &Filter {
                    query: String::new(),
                    oldest: true,
                    ..Default::default()
                }
            ),
            vec![PathBuf::from("old.mp4"), PathBuf::from("new.mp4")]
        );
        assert_eq!(
            filtered(
                files,
                &map,
                &Filter {
                    query: "old".into(),
                    oldest: false,
                    ..Default::default()
                }
            ),
            vec![PathBuf::from("old.mp4")]
        );
    }
    #[test]
    fn damaged_metadata_is_not_overwritten() {
        let d = std::env::temp_dir().join(format!("zim-tags-bad-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let db = d.join(".zimclip-tags.tsv");
        std::fs::write(&db, "damaged").unwrap();
        assert!(set_tags(&d.join("a.mp4"), "good").is_err());
        assert_eq!(std::fs::read_to_string(db).unwrap(), "damaged");
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn trash_sidecar_keeps_tags() {
        let d = std::env::temp_dir().join(format!("zim-tags-trash-{}", std::process::id()));
        std::fs::create_dir_all(d.join("Trash")).unwrap();
        let a = d.join("a.mp4");
        let b = d.join("Trash").join("123-a.mp4");
        set_tags(&a, "Friends").unwrap();
        trash_metadata(&a, &b).unwrap();
        assert_eq!(
            std::fs::read_to_string(b.with_extension("mp4.tags.txt")).unwrap(),
            "Friends"
        );
        std::fs::remove_dir_all(d).unwrap();
    }
}

pub fn rename_palette_tag(dir: &Path, old: &str, new: &str, color: u32) -> Result<(), String> {
    if normalize(new)? != new || new.is_empty() || new.contains(',') || color > 0xffffff {
        return Err("Invalid tag name or colour".into());
    }
    if old == new {
        return save_color(dir, old, color);
    }
    let mut palette = reusable(dir)?;
    if palette
        .keys()
        .any(|k| k.eq_ignore_ascii_case(new) && !k.eq_ignore_ascii_case(old))
    {
        return Err("That tag name already exists".into());
    }
    let mut assignments = read_tags(dir)?;
    for value in assignments.values_mut() {
        *value = value
            .split(',')
            .map(str::trim)
            .map(|t| if t.eq_ignore_ascii_case(old) { new } else { t })
            .collect::<Vec<_>>()
            .join(", ");
    }
    let backup = std::fs::read(dir.join(".zimclip-palette.tsv")).ok();
    palette.retain(|k, _| !k.eq_ignore_ascii_case(old));
    palette.insert(new.into(), color);
    let content = palette
        .iter()
        .map(|(t, c)| format!("{}\t{:06x}\n", hex(t), c))
        .collect::<String>();
    // Palette first, with rollback if assignment write fails. Videos are untouched.
    let path = dir.join(".zimclip-palette.tsv");
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    if let Err(e) = store(dir, &assignments) {
        match backup {
            Some(b) => {
                let _ = std::fs::write(&path, b);
            }
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
        return Err(e);
    }
    Ok(())
}
#[cfg(test)]
mod palette_edit_tests {
    use super::*;
    #[test]
    fn rename_preserves_clip_bytes_and_assignments() {
        let d = std::env::temp_dir().join(format!("zim-palette-edit-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("video.mp4");
        std::fs::write(&p, b"original-video").unwrap();
        set_tags(&p, "Friends, Game").unwrap();
        save_color(&d, "Friends", PALETTE[3]).unwrap();
        rename_palette_tag(&d, "Friends", "Buddies", PALETTE[4]).unwrap();
        assert_eq!(tags(&d)["video.mp4"], "Buddies, Game");
        assert!(!reusable(&d).unwrap().contains_key("Friends"));
        assert_eq!(std::fs::read(p).unwrap(), b"original-video");
        std::fs::remove_dir_all(d).unwrap();
    }
}

pub fn inherit(source: &Path, copy: &Path) -> Result<(), String> {
    let dir = source.parent().ok_or("Missing source folder")?;
    if let Some(value) = read_tags(dir)?.get(&key(source)) {
        set_tags(copy, value)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod inherit_tests {
    use super::*;
    #[test]
    fn edited_copy_keeps_tags_and_palette() {
        let d = std::env::temp_dir().join(format!("zim-inherit-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let a = d.join("a.mp4");
        let b = d.join("sized-copy.mp4");
        std::fs::write(&a, b"original").unwrap();
        std::fs::write(&b, b"copy").unwrap();
        set_tags(&a, "Ballers, Bush").unwrap();
        save_color(&d, "Ballers", PALETTE[3]).unwrap();
        inherit(&a, &b).unwrap();
        let m = read_tags(&d).unwrap();
        assert_eq!(m["a.mp4"], m["sized-copy.mp4"]);
        let f = Filter {
            tag: "Ballers".into(),
            ..Default::default()
        };
        assert_eq!(filtered(vec![a.clone(), b], &m, &f).len(), 2);
        assert_eq!(std::fs::read(a).unwrap(), b"original");
        std::fs::remove_dir_all(d).unwrap();
    }
}

/// Number of clips that carry `tag` (case-insensitive).
pub fn tag_use_count(dir: &Path, tag: &str) -> usize {
    tags(dir)
        .values()
        .filter(|v| v.split(',').any(|t| t.trim().to_lowercase() == tag.to_lowercase()))
        .count()
}
/// Remove a tag from the palette and from every clip's tag list. Video files are never touched.
/// The palette is written first and restored if the assignment write fails.
pub fn delete_palette_tag(dir: &Path, tag: &str) -> Result<(), String> {
    if tag.is_empty() || tag.contains(',') {
        return Err("Invalid tag name".into());
    }
    let mut palette = reusable(dir)?;
    let mut assignments = read_tags(dir)?;
    for value in assignments.values_mut() {
        *value = value
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty() && t.to_lowercase() != tag.to_lowercase())
            .collect::<Vec<_>>()
            .join(", ");
    }
    assignments.retain(|_, v| !v.is_empty());
    palette.retain(|k, _| k.to_lowercase() != tag.to_lowercase());
    let path = dir.join(".zimclip-palette.tsv");
    let backup = std::fs::read_to_string(&path).ok();
    let content = palette
        .iter()
        .map(|(t, c)| format!("{}\t{:06x}\n", hex(t), c))
        .collect::<String>();
    write_palette_text(dir, &content)?;
    if let Err(e) = store(dir, &assignments) {
        match backup {
            Some(b) => {
                let _ = write_palette_text(dir, &b);
            }
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
        return Err(e);
    }
    Ok(())
}
#[cfg(test)]
mod palette_delete_tests {
    use super::*;
    #[test]
    fn delete_removes_tag_everywhere_and_keeps_videos() {
        let d = std::env::temp_dir().join(format!("zim-palette-del-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("a.mp4"), d.join("b.mp4"));
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        set_tags(&a, "Friends, Game").unwrap();
        set_tags(&b, "friends").unwrap();
        save_color(&d, "Friends", PALETTE[3]).unwrap();
        assert_eq!(tag_use_count(&d, "friends"), 2);
        delete_palette_tag(&d, "Friends").unwrap();
        assert_eq!(tags(&d)["a.mp4"], "Game");
        assert!(!tags(&d).contains_key("b.mp4"));
        let r = reusable(&d).unwrap();
        assert!(r.contains_key("Game") && !r.keys().any(|k| k.eq_ignore_ascii_case("friends")));
        assert_eq!(std::fs::read(&a).unwrap(), b"A");
        assert_eq!(std::fs::read(&b).unwrap(), b"B");
        assert_eq!(tag_use_count(&d, "Friends"), 0);
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn non_ascii_case_variants_are_counted_and_deleted_together() {
        let d = std::env::temp_dir().join(format!("zim-palette-del-u-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("a.mp4"), d.join("b.mp4"));
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        set_tags(&a, "Élan, Game").unwrap();
        set_tags(&b, "éLAN").unwrap();
        save_color(&d, "Élan", PALETTE[2]).unwrap();
        assert_eq!(tag_use_count(&d, "Élan"), 2);
        assert_eq!(tag_use_count(&d, "élan"), 2);
        delete_palette_tag(&d, "Élan").unwrap();
        assert_eq!(tags(&d)["a.mp4"], "Game");
        assert!(!tags(&d).contains_key("b.mp4"));
        assert!(!reusable(&d).unwrap().keys().any(|k| k.to_lowercase() == "élan"));
        assert_eq!(tag_use_count(&d, "Élan"), 0);
        std::fs::remove_dir_all(d).unwrap();
    }
}
