use std::{
    collections::VecDeque,
    io::{self, Read},
    time::{Duration, Instant},
};
const MAX_BYTES: usize = 48 * 1024 * 1024;
#[derive(Clone)]
pub struct Fragment {
    pub at: Instant,
    pub data: Vec<u8>,
}
pub struct Ring {
    seconds: u64,
    max_bytes: usize,
    pub init: Vec<u8>,
    pub parts: VecDeque<Fragment>,
    pub bytes: usize,
    pub error: Option<String>,
}
impl Ring {
    pub fn new() -> Self {
        Self::with_seconds(30)
    }
    pub fn with_seconds(seconds: u64) -> Self {
        let seconds = seconds.clamp(5, 300);
        Self {
            seconds,
            max_bytes: ((seconds as usize) * 768 * 1024).clamp(MAX_BYTES, 192 * 1024 * 1024),
            init: Vec::new(),
            parts: VecDeque::new(),
            bytes: 0,
            error: None,
        }
    }
    pub fn push(&mut self, data: Vec<u8>, now: Instant) {
        self.bytes += data.len();
        self.parts.push_back(Fragment { at: now, data });
        while self.parts.len() > 1
            && (now.duration_since(self.parts.front().unwrap().at)
                >= Duration::from_secs(self.seconds)
                || self.parts.len() > self.seconds as usize
                || self.bytes > self.max_bytes)
        {
            self.bytes -= self.parts.pop_front().unwrap().data.len();
        }
    }
    pub fn snapshot(&self) -> Option<Vec<u8>> {
        if self.init.is_empty() || self.parts.is_empty() {
            return None;
        }
        let mut data = Vec::with_capacity(self.init.len() + self.bytes);
        data.extend_from_slice(&self.init);
        for p in &self.parts {
            data.extend_from_slice(&p.data);
        }
        Some(data)
    }
}
pub fn read_box(r: &mut impl Read) -> io::Result<([u8; 4], Vec<u8>)> {
    let mut h = [0u8; 8];
    r.read_exact(&mut h)?;
    let mut n = u32::from_be_bytes(h[..4].try_into().unwrap()) as u64;
    let typ = h[4..8].try_into().unwrap();
    let mut v = h.to_vec();
    if n == 1 {
        let mut ext = [0u8; 8];
        r.read_exact(&mut ext)?;
        n = u64::from_be_bytes(ext);
        v.extend_from_slice(&ext);
    }
    if n < v.len() as u64 || n > MAX_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unexpected capture box size",
        ));
    }
    let old = v.len();
    v.resize(n as usize, 0);
    r.read_exact(&mut v[old..])?;
    Ok((typ, v))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evicts_by_time() {
        let mut r = Ring::new();
        let t = Instant::now();
        for i in 0..45 {
            r.push(vec![i as u8; 4], t + Duration::from_secs(i));
        }
        assert_eq!(r.parts.len(), 30);
        assert_eq!(r.parts.front().unwrap().data[0], 15);
        assert_eq!(r.bytes, 120);
    }
    #[test]
    fn caps_bursted_fragments() {
        let mut r = Ring::new();
        let t = Instant::now();
        for _ in 0..50 {
            r.push(vec![0], t);
        }
        assert_eq!(r.parts.len(), 30);
    }
    #[test]
    fn custom_duration() {
        for seconds in [5, 15, 60, 120, 300] {
            let mut r = Ring::with_seconds(seconds);
            let t = Instant::now();
            for i in 0..seconds + 10 {
                r.push(vec![0; 4], t + Duration::from_secs(i));
            }
            assert_eq!(r.parts.len(), seconds as usize);
        }
    }
    #[test]
    fn memory_cap() {
        let mut r = Ring::with_seconds(5);
        let t = Instant::now();
        for _ in 0..5 {
            r.push(vec![0; 16 * 1024 * 1024], t);
        }
        assert!(r.bytes <= 48 * 1024 * 1024);
    }
    #[test]
    fn snapshot_keeps_init() {
        let mut r = Ring::new();
        r.init = vec![1, 2];
        r.push(vec![3, 4], Instant::now());
        assert_eq!(r.snapshot().unwrap(), vec![1, 2, 3, 4]);
    }
    #[test]
    fn rejects_bad_box() {
        assert!(read_box(&mut &b"\0\0\0\0mdat"[..]).is_err());
    }
    #[test]
    fn reads_box() {
        let (t, b) = read_box(&mut &b"\0\0\0\x0cmdatDATA"[..]).unwrap();
        assert_eq!(&t, b"mdat");
        assert_eq!(b.len(), 12);
    }
}
