use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
pub fn duration(path: &Path) -> io::Result<f64> {
    let mut f = File::open(path)?;
    let end = f.metadata()?.len();
    scan(&mut f, end, false)
}
fn scan(f: &mut File, end: u64, inside: bool) -> io::Result<f64> {
    while f.stream_position()? + 8 <= end {
        let pos = f.stream_position()?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h)?;
        let mut size = u32::from_be_bytes(h[..4].try_into().unwrap()) as u64;
        let mut header = 8;
        if size == 1 {
            let mut n = [0; 8];
            f.read_exact(&mut n)?;
            size = u64::from_be_bytes(n);
            header = 16;
        }
        if size == 0 {
            size = end - pos;
        }
        if size < header || size > end - pos {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid MP4 atom size",
            ));
        }
        if &h[4..] == b"moov" {
            if let Ok(v) = scan(f, pos + size, true) {
                return Ok(v);
            }
        }
        if inside && &h[4..] == b"mvhd" {
            let mut b = [0; 32];
            let n = ((size - header) as usize).min(32);
            f.read_exact(&mut b[..n])?;
            let (scale, ticks) = if b[0] == 1 && n >= 32 {
                (
                    u32::from_be_bytes(b[20..24].try_into().unwrap()),
                    u64::from_be_bytes(b[24..32].try_into().unwrap()),
                )
            } else if b[0] == 0 && n >= 20 {
                (
                    u32::from_be_bytes(b[12..16].try_into().unwrap()),
                    u32::from_be_bytes(b[16..20].try_into().unwrap()) as u64,
                )
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid movie duration",
                ));
            };
            if scale > 0 && ticks > 0 {
                return Ok(ticks as f64 / scale as f64);
            }
        }
        f.seek(SeekFrom::Start(pos + size))?;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "MP4 duration not found",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_duration() {
        let p = std::env::temp_dir().join("zimclip-duration-unit.mp4");
        let mut b = Vec::new();
        b.extend(36u32.to_be_bytes());
        b.extend(b"moov");
        b.extend(28u32.to_be_bytes());
        b.extend(b"mvhd");
        b.extend([0u8; 12]);
        b.extend(1000u32.to_be_bytes());
        b.extend(30000u32.to_be_bytes());
        std::fs::write(&p, b).unwrap();
        assert_eq!(duration(&p).unwrap(), 30.0);
        std::fs::remove_file(p).unwrap();
    }
}

// Track order is the stream order for our MP4 captures. This avoids relying
// on an optional MF stream-descriptor attribute for the major media type.
pub fn track_kinds(path: &Path) -> Option<Vec<[u8; 4]>> {
    let mut f = File::open(path).ok()?;
    let end = f.metadata().ok()?.len();
    while f.stream_position().ok()? + 8 <= end {
        let pos = f.stream_position().ok()?;
        let mut h = [0; 8];
        f.read_exact(&mut h).ok()?;
        let n = u32::from_be_bytes(h[..4].try_into().ok()?) as u64;
        if n < 8 || n > end - pos {
            return None;
        }
        if &h[4..] == b"moov" && n < 16 * 1024 * 1024 {
            let mut b = vec![0; n as usize - 8];
            f.read_exact(&mut b).ok()?;
            let mut kinds = Vec::new();
            fn children<'a>(mut b: &'a [u8], target: &[u8; 4]) -> Vec<&'a [u8]> {
                let mut out = Vec::new();
                while b.len() >= 8 {
                    let n = u32::from_be_bytes(b[..4].try_into().unwrap()) as usize;
                    if n < 8 || n > b.len() {
                        break;
                    }
                    if &b[4..8] == target {
                        out.push(&b[8..n]);
                    }
                    b = &b[n..];
                }
                out
            }
            for trak in children(&b, b"trak") {
                let mdia = children(trak, b"mdia");
                let h = children(mdia.first()?, b"hdlr");
                let h = *h.first()?;
                if h.len() < 24 {
                    return None;
                }
                kinds.push(h[8..12].try_into().ok()?);
            }
            return Some(kinds);
        }
        f.seek(SeekFrom::Start(pos + n)).ok()?;
    }
    None
}
pub fn audio_selection(kinds: &[[u8; 4]]) -> Vec<Option<bool>> {
    let mut first = true;
    kinds
        .iter()
        .map(|k| {
            if k == b"soun" {
                let selected = first;
                first = false;
                Some(selected)
            } else {
                None
            }
        })
        .collect()
}
pub fn has_separate_audio(path: &Path) -> bool {
    fn child<'a>(mut bytes: &'a [u8], target: &[u8; 4]) -> Option<&'a [u8]> {
        while bytes.len() >= 8 {
            let n = u32::from_be_bytes(bytes[..4].try_into().ok()?) as usize;
            if n < 8 || n > bytes.len() {
                return None;
            }
            if &bytes[4..8] == target {
                return Some(&bytes[8..n]);
            }
            bytes = &bytes[n..];
        }
        None
    }
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let Ok(end) = f.metadata().map(|m| m.len()) else {
        return false;
    };
    while let Ok(pos) = f.stream_position() {
        if pos + 8 > end {
            break;
        }
        let mut h = [0u8; 8];
        if f.read_exact(&mut h).is_err() {
            break;
        }
        let n = u32::from_be_bytes(h[..4].try_into().unwrap()) as u64;
        if n < 8 || n > end - pos {
            break;
        }
        if &h[4..] == b"moov" && n < 16 * 1024 * 1024 {
            let mut moov = vec![0; n as usize - 8];
            if f.read_exact(&mut moov).is_err() {
                return false;
            }
            let mut b = moov.as_slice();
            let mut names = Vec::new();
            while b.len() >= 8 {
                let size = u32::from_be_bytes(b[..4].try_into().unwrap()) as usize;
                if size < 8 || size > b.len() {
                    break;
                }
                if &b[4..8] == b"trak" {
                    if let Some(h) = child(&b[8..size], b"mdia").and_then(|m| child(m, b"hdlr")) {
                        if h.len() >= 24 && &h[8..12] == b"soun" {
                            names.push(
                                String::from_utf8_lossy(&h[24..])
                                    .trim_end_matches('\0')
                                    .to_string(),
                            );
                        }
                    }
                }
                b = &b[size..];
            }
            return names
                == [
                    "ZimClip Mix",
                    "ZimClip System",
                    "ZimClip Mic",
                    "ZimClip Comms",
                ];
        }
        if f.seek(SeekFrom::Start(pos + n)).is_err() {
            break;
        }
    }
    false
}

#[cfg(test)]
mod audio_tests {
    use super::*;
    fn atom(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend(name);
        v.extend(payload);
        v
    }
    #[test]
    fn named_audio_detection() {
        let p = std::env::temp_dir().join(format!("zc-audio-detect-{}.mp4", std::process::id()));
        let mut tracks = Vec::new();
        for name in [
            "ZimClip Mix",
            "ZimClip System",
            "ZimClip Mic",
            "ZimClip Comms",
        ] {
            let mut h = vec![0; 24];
            h[8..12].copy_from_slice(b"soun");
            h.extend(name.as_bytes());
            h.push(0);
            tracks.extend(atom(b"trak", &atom(b"mdia", &atom(b"hdlr", &h))));
        }
        std::fs::write(&p, atom(b"moov", &tracks)).unwrap();
        assert!(has_separate_audio(&p));
        std::fs::write(&p, atom(b"moov", &[])).unwrap();
        assert!(!has_separate_audio(&p));
        std::fs::remove_file(p).unwrap();
    }
}

#[cfg(test)]
mod playback_tests {
    use super::*;
    #[test]
    fn first_audio_only() {
        assert_eq!(
            audio_selection(&[*b"vide", *b"soun", *b"soun", *b"soun", *b"soun"]),
            vec![None, Some(true), Some(false), Some(false), Some(false)]
        );
        assert_eq!(
            audio_selection(&[*b"soun", *b"vide"]),
            vec![Some(true), None]
        );
        assert_eq!(audio_selection(&[*b"vide"]), vec![None]);
    }
}

// None means natural end. Do not feed the source movie duration into an
// audition whose native presentation can end a few milliseconds earlier.
pub fn preview_stop_ticks(enabled: bool, end: f64, native: Option<i64>) -> Option<i64> {
    if !enabled || !end.is_finite() || end <= 0.0 {
        return None;
    }
    let requested = (end * 10_000_000.0).round() as i64;
    native
        .filter(|limit| requested > 0 && requested < limit.saturating_sub(10_000))
        .map(|_| requested)
}
/// Playback waits on Media Foundation callbacks. If none arrives within this
/// time the editor starts a new player session so Play is never refused forever.
/// Play from a paused player can skip setting the same item again only when the bounds known to
/// be on the loaded item are exactly the wanted ones. Unknown bounds (None) always set up again.
pub fn can_skip_rebind(paused: bool, have: Option<(bool, u64, u64)>, want: (bool, u64, u64)) -> bool {
    paused && have == Some(want)
}
/// A seek on a paused player can go straight to the loaded item when the target lies inside the
/// bounds that are really set on it (`have`, the key recorded when they were set; edges inclusive).
/// Unknown or non-trim bounds, a changed range, or a target that is not a finite number keep the
/// old path: clear the bounds first.
pub fn paused_seek_needs_bounds_clear(have: Option<(bool, u64, u64)>, target: f64) -> bool {
    match have {
        Some((true, s, e)) => {
            let (s, e) = (f64::from_bits(s), f64::from_bits(e));
            !(target.is_finite() && s.is_finite() && e.is_finite() && s <= e && target >= s && target <= e)
        }
        _ => true,
    }
}
/// When the next coalesced seek may be sent. Outside a drag it keeps the 160 ms spacing. While
/// dragging, a new seek goes out as soon as the previous one was reported (at least 30 ms
/// apart), so the preview follows the pointer without queueing seeks; if the player never
/// reports one, the next goes out after 400 ms.
pub fn seek_due(since_last: std::time::Duration, dragging: bool, inflight: Option<std::time::Duration>) -> bool {
    use std::time::Duration as D;
    if !dragging {
        return since_last >= D::from_millis(160);
    }
    since_last >= D::from_millis(30) && inflight.map_or(true, |i| i >= D::from_millis(400))
}
pub fn playback_wait_expired(waited: std::time::Duration) -> bool {
    waited >= std::time::Duration::from_secs(10)
}
/// Callback channel of the current player session. `fresh` detaches the previous
/// session: its queued events are dropped and its sender can no longer deliver.
pub struct PlaybackSessions {
    rx: std::sync::mpsc::Receiver<(i32, i32)>,
}
impl Default for PlaybackSessions {
    fn default() -> Self {
        Self { rx: std::sync::mpsc::channel().1 }
    }
}
impl PlaybackSessions {
    pub fn fresh(&mut self) -> std::sync::mpsc::Sender<(i32, i32)> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = rx;
        tx
    }
    pub fn try_recv(&mut self) -> Option<(i32, i32)> {
        self.rx.try_recv().ok()
    }
}
#[cfg(test)]
mod rebind_tests {
    use super::*;
    #[test]
    fn rebind_is_skipped_only_for_paused_player_with_identical_bounds() {
        let k = (true, 4, 5);
        assert!(can_skip_rebind(true, Some(k), k));
        assert!(!can_skip_rebind(false, Some(k), k));
        assert!(!can_skip_rebind(true, None, k));
        // A changed stop, a changed start or a different mode each force the full setup.
        assert!(!can_skip_rebind(true, Some((true, 4, 6)), k));
        assert!(!can_skip_rebind(true, Some((true, 3, 5)), k));
        assert!(!can_skip_rebind(true, Some((false, 4, 5)), k));
    }
    #[test]
    fn paused_seek_clears_bounds_only_outside_the_loaded_range() {
        let k = Some((true, 2.0f64.to_bits(), 8.0f64.to_bits()));
        // Inside, and both edges inclusive: no clear.
        assert!(!paused_seek_needs_bounds_clear(k, 5.0));
        assert!(!paused_seek_needs_bounds_clear(k, 2.0));
        assert!(!paused_seek_needs_bounds_clear(k, 8.0));
        // Outside either edge: clear.
        assert!(paused_seek_needs_bounds_clear(k, 1.999));
        assert!(paused_seek_needs_bounds_clear(k, 8.001));
        // Unknown, non-trim or invalid loaded bounds, and non-finite targets: old path.
        assert!(paused_seek_needs_bounds_clear(None, 5.0));
        assert!(paused_seek_needs_bounds_clear(Some((false, 2.0f64.to_bits(), 8.0f64.to_bits())), 5.0));
        assert!(paused_seek_needs_bounds_clear(Some((true, 8.0f64.to_bits(), 2.0f64.to_bits())), 5.0));
        assert!(paused_seek_needs_bounds_clear(Some((true, f64::NAN.to_bits(), 8.0f64.to_bits())), 5.0));
        assert!(paused_seek_needs_bounds_clear(k, f64::NAN));
        assert!(paused_seek_needs_bounds_clear(k, f64::INFINITY));
        // The loaded range decides, not a newer edited start: with start edited to 6 after load,
        // the key is still (2, 8), so a target of 3 is inside what is loaded.
        assert!(!paused_seek_needs_bounds_clear(k, 3.0));
    }
}
#[cfg(test)]
mod seek_due_tests {
    use super::*;
    use std::time::Duration as D;
    #[test]
    fn drag_seeks_wait_for_the_previous_one() {
        assert!(!seek_due(D::from_millis(159), false, None));
        assert!(seek_due(D::from_millis(160), false, None));
        assert!(!seek_due(D::from_millis(10), true, None));
        assert!(seek_due(D::from_millis(30), true, None));
        assert!(!seek_due(D::from_millis(200), true, Some(D::from_millis(50))));
        assert!(seek_due(D::from_millis(500), true, Some(D::from_millis(400))));
    }
}
#[cfg(test)]
mod playback_session_tests {
    use super::*;
    #[test]
    fn old_session_events_never_reach_new_session() {
        let mut s = PlaybackSessions::default();
        let old = s.fresh();
        old.send((1, 0)).unwrap();
        let new = s.fresh();
        assert!(old.send((2, 0)).is_err());
        assert_eq!(s.try_recv(), None);
        new.send((3, 0)).unwrap();
        assert_eq!(s.try_recv(), Some((3, 0)));
        assert_eq!(s.try_recv(), None);
    }
    #[test]
    fn missing_callbacks_yield_nothing_and_repeat_recovery_isolated() {
        let mut s = PlaybackSessions::default();
        let a = s.fresh();
        assert_eq!(s.try_recv(), None);
        let b = s.fresh();
        let c = s.fresh();
        assert!(a.send((1, 0)).is_err());
        assert!(b.send((2, 0)).is_err());
        c.send((9, 0)).unwrap();
        assert_eq!(s.try_recv(), Some((9, 0)));
    }
    #[test]
    fn wait_limit() {
        assert!(!playback_wait_expired(std::time::Duration::from_millis(9900)));
        assert!(playback_wait_expired(std::time::Duration::from_secs(10)));
    }
}
/// Space starts or pauses playback when the key goes to the timeline, to the Play button, or to
/// the editor window itself (nothing focused, as right after opening). Other controls keep Space.
pub fn space_starts_play(on_timeline: bool, on_editor_window: bool, play_focused: bool) -> bool {
    on_timeline || on_editor_window || play_focused
}
#[cfg(test)]
mod space_routing_tests {
    use super::*;
    #[test]
    fn space_routing() {
        assert!(space_starts_play(true, false, false));
        assert!(space_starts_play(false, true, false)); // nothing focused after open
        assert!(space_starts_play(false, false, true));
        assert!(!space_starts_play(false, false, false)); // combo, edit, other button
    }
}
#[cfg(test)]
mod preview_bounds_tests {
    use super::*;
    #[test]
    fn source_longer_than_audition_uses_natural_end() {
        assert_eq!(preview_stop_ticks(true, 30.011, Some(300_000_000)), None);
    }
    #[test]
    fn interior_trim_uses_native_stop() {
        assert_eq!(
            preview_stop_ticks(true, 12.0, Some(300_000_000)),
            Some(120_000_000)
        );
    }
    #[test]
    fn full_end_disabled_unknown_and_invalid_are_unbounded() {
        for (enabled, end, native) in [
            (true, 30.0, Some(300_000_000)),
            (false, 12.0, Some(300_000_000)),
            (true, 12.0, None),
            (true, f64::NAN, Some(300_000_000)),
            (true, -1.0, Some(300_000_000)),
        ] {
            assert_eq!(preview_stop_ticks(enabled, end, native), None);
        }
    }
}
