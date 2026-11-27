// Four stereo buses share one sample clock: playback mix, system, mic, comms.
pub fn pack(system: &[u8], mic: &[u8], comms: &[u8], out: &mut Vec<u8>) {
    assert_eq!(system.len(), mic.len());
    assert_eq!(system.len(), comms.len());
    assert_eq!(system.len() % 4, 0);
    out.clear();
    out.reserve(system.len() * 4);
    for ((s, m), c) in system
        .chunks_exact(4)
        .zip(mic.chunks_exact(4))
        .zip(comms.chunks_exact(4))
    {
        for channel in 0..2 {
            let i = channel * 2;
            let a = i16::from_le_bytes([s[i], s[i + 1]]) as i32;
            let b = i16::from_le_bytes([m[i], m[i + 1]]) as i32;
            let d = i16::from_le_bytes([c[i], c[i + 1]]) as i32;
            out.extend_from_slice(&((a + b + d).clamp(-32112, 32112) as i16).to_le_bytes());
        }
        out.extend_from_slice(s);
        out.extend_from_slice(m);
        out.extend_from_slice(c);
    }
}
pub fn capture_graph() -> &'static str {
    "[0:a]asplit=4[b0][b1][b2][b3];[b0]pan=stereo|c0=c0|c1=c1[mix];[b1]pan=stereo|c0=c2|c1=c3[system];[b2]pan=stereo|c0=c4|c1=c5[mic];[b3]pan=stereo|c0=c6|c1=c7[comms]"
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_buses() {
        let mut out = Vec::new();
        pack(&[10, 0, 20, 0], &[30, 0, 40, 0], &[50, 0, 60, 0], &mut out);
        let values: Vec<_> = out
            .chunks_exact(2)
            .map(|p| i16::from_le_bytes([p[0], p[1]]))
            .collect();
        assert_eq!(values, vec![90, 120, 10, 20, 30, 40, 50, 60]);
    }
    #[test]
    fn mix_clamps_without_changing_sources() {
        let bus = [255, 127, 0, 128];
        let mut out = Vec::new();
        pack(&bus, &bus, &bus, &mut out);
        assert_eq!(&out[4..8], &bus);
        assert_eq!(&out[..4], &[112, 125, 144, 130]);
    }
}
