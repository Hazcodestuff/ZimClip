//! Animation maths shared by the page transitions. Platform neutral so it is unit tested.
/// Fast start, gentle stop. 0 at t=0, 1 at t>=1.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    1. - (1. - t) * (1. - t) * (1. - t)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ease_is_bounded_monotone_and_hits_both_ends() {
        let mut last = 0.;
        for i in 0..=200 {
            let e = ease_out(i as f32 / 200.);
            assert!((0.0..=1.0).contains(&e) && e >= last);
            last = e;
        }
        assert_eq!(ease_out(-1.), 0.);
        assert_eq!(ease_out(0.), 0.);
        assert_eq!(ease_out(1.), 1.);
        assert_eq!(ease_out(5.), 1.);
        // Ease-out: more than half the move is done by the halfway point.
        assert!(ease_out(0.5) > 0.8);
    }
}
