use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
#[derive(Default)]
pub struct Selection {
    pub active: bool,
    pub paths: BTreeSet<PathBuf>,
}
pub fn state() -> &'static Mutex<Selection> {
    static S: OnceLock<Mutex<Selection>> = OnceLock::new();
    S.get_or_init(Default::default)
}
pub fn selected(p: &Path) -> bool {
    state().lock().unwrap().paths.contains(p)
}
pub fn toggle(p: PathBuf) {
    let mut s = state().lock().unwrap();
    s.active = true;
    if !s.paths.remove(&p) {
        s.paths.insert(p);
    }
}
pub fn trash(p: &Path, dir: &Path) -> Result<PathBuf, String> {
    if !p.is_file()
        || p.file_name().is_some_and(|n| {
            n.to_string_lossy().ends_with(".partial.mp4")
                || n.to_string_lossy().ends_with(".recoverable.mp4")
        })
        || p.parent() != Some(dir)
        || p.extension().is_none_or(|e| !e.eq_ignore_ascii_case("mp4"))
    {
        return Err("Not a clip in this library.".into());
    }
    let folder = dir.join("Trash");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let name = p.file_name().ok_or("Missing clip name")?;
    let dest = folder.join(format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        name.to_string_lossy()
    ));
    if dest.exists() {
        return Err("Trash destination already exists.".into());
    }
    crate::catalog::trash_metadata(p, &dest)?;
    std::fs::rename(p, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tagged_trash_restore_preserves_bytes_and_tags() {
        let d = std::env::temp_dir().join(format!("zim-trash-tags-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let a = d.join("my-clip.mp4");
        std::fs::write(&a, b"unchanged-video-bytes").unwrap();
        crate::catalog::set_tags(&a, "Friends, Win").unwrap();
        let moved = trash(&a, &d).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(&moved).unwrap(), b"unchanged-video-bytes");
        assert_eq!(
            std::fs::read_to_string(moved.with_extension("mp4.tags.txt")).unwrap(),
            "Friends, Win"
        );
        std::fs::rename(&moved, &a).unwrap();
        assert_eq!(
            crate::catalog::tags(&d).get("my-clip.mp4").unwrap(),
            "Friends, Win"
        );
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn trash_moves_only_target() {
        let d = std::env::temp_dir().join(format!("zim-trash-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let a = d.join("a.mp4");
        let b = d.join("b.mp4");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let dest = trash(&a, &d).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(dest).unwrap(), b"a");
        assert!(b.exists());
        assert!(trash(&b, &d.join("wrong")).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }
}
