pub struct Meter {
    pub peak: f64,
    pub sum: f64,
    pub samples: u64,
    pub limited: u64,
}
impl Meter {
    pub fn new() -> Self {
        Self {
            peak: 0.0,
            sum: 0.0,
            samples: 0,
            limited: 0,
        }
    }
    pub fn process(&mut self, bytes: &mut [u8], gain_db: i32) {
        let gain = 10f64.powf(gain_db as f64 / 20.0);
        for p in bytes.chunks_exact_mut(2) {
            let x = i16::from_le_bytes([p[0], p[1]]) as f64 / 32768.0;
            self.peak = self.peak.max(x.abs());
            self.sum += x * x;
            self.samples += 1;
            let y = x * gain;
            if gain_db > 0 && y.abs() > 0.98 {
                self.limited += 1;
            }
            let y = (if gain_db > 0 { y.clamp(-0.98, 0.98) } else { x } * 32768.0).round() as i16;
            p.copy_from_slice(&y.to_le_bytes());
        }
    }
    pub fn db(&self) -> (f64, f64) {
        (
            20.0 * self.peak.max(1e-6).log10(),
            20.0 * (self.sum / self.samples.max(1) as f64)
                .sqrt()
                .max(1e-6)
                .log10(),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unity() {
        let mut m = Meter::new();
        let mut b = 1000i16.to_le_bytes();
        m.process(&mut b, 0);
        assert_eq!(i16::from_le_bytes(b), 1000);
    }
    #[test]
    fn boost() {
        let mut m = Meter::new();
        let mut b = 1000i16.to_le_bytes();
        m.process(&mut b, 6);
        assert!((i16::from_le_bytes(b) - 1995).abs() <= 1);
    }
    #[test]
    fn bounds() {
        let mut m = Meter::new();
        let mut b = 30000i16.to_le_bytes();
        m.process(&mut b, 24);
        assert!(i16::from_le_bytes(b) < 32767);
        assert_eq!(m.limited, 1);
    }
}

pub fn mix_pcm(system: &mut [u8], mic: &[u8], gain_db: i32) {
    let gain = 10f64.powf(gain_db as f64 / 20.0);
    for (s, m) in system.chunks_exact_mut(2).zip(mic.chunks_exact(2)) {
        let a = i16::from_le_bytes([s[0], s[1]]) as f64;
        let b = i16::from_le_bytes([m[0], m[1]]) as f64 * gain;
        let v = (a + b).clamp(-32112.0, 32112.0).round() as i16;
        s.copy_from_slice(&v.to_le_bytes());
    }
}
#[cfg(test)]
mod mixing_tests {
    use super::*;
    #[test]
    fn sums() {
        let mut a = 1000i16.to_le_bytes();
        mix_pcm(&mut a, &500i16.to_le_bytes(), 0);
        assert_eq!(i16::from_le_bytes(a), 1500);
    }
    #[test]
    fn saturates() {
        let mut a = 30000i16.to_le_bytes();
        mix_pcm(&mut a, &30000i16.to_le_bytes(), 0);
        assert_eq!(i16::from_le_bytes(a), 32112);
    }
    #[test]
    fn negative() {
        let mut a = (-30000i16).to_le_bytes();
        mix_pcm(&mut a, &(-30000i16).to_le_bytes(), 0);
        assert_eq!(i16::from_le_bytes(a), -32112);
    }
}
