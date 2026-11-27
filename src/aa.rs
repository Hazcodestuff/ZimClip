//! Anti-aliased shapes for the setup window, computed on the CPU as premultiplied BGRA.
//! GDI's own lines and rounded rectangles are not anti-aliased, which shows as stair-steps
//! on thin curves. Pure code, so Linux tests can inspect the actual pixels.

pub struct Px {
    pub w: usize,
    pub h: usize,
    /// Premultiplied BGRA, row by row.
    pub data: Vec<u8>,
}
fn rgb(c: u32) -> [f32; 3] {
    [(c & 255) as f32, (c >> 8 & 255) as f32, (c >> 16 & 255) as f32]
}
fn put(px: &mut Px, x: usize, y: usize, c: [f32; 3], a: f32) {
    let i = (y * px.w + x) * 4;
    let a = a.clamp(0., 1.);
    // c is RGB; store BGRA premultiplied.
    px.data[i] = (c[2] * a).round() as u8;
    px.data[i + 1] = (c[1] * a).round() as u8;
    px.data[i + 2] = (c[0] * a).round() as u8;
    px.data[i + 3] = (a * 255.).round() as u8;
}
fn cov(d: f32) -> f32 {
    (0.5 - d).clamp(0., 1.)
}
/// Signed distance to a rounded box centered at the origin (negative inside).
fn sd_box(x: f32, y: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = x.abs() - hw + r;
    let qy = y.abs() - hh + r;
    qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - r
}

/// Rounded rectangle w x h with corner radius `radius` and a 1px inner outline.
pub fn rrect(w: usize, h: usize, radius: i32, fill: Option<u32>, stroke: Option<u32>) -> Px {
    let mut px = Px { w, h, data: vec![0; w * h * 4] };
    let r = (radius as f32).min(w.min(h) as f32 / 2.);
    let (hw, hh) = (w as f32 / 2., h as f32 / 2.);
    let line = stroke.or(fill);
    for y in 0..h {
        for x in 0..w {
            let d = sd_box(x as f32 + 0.5 - hw, y as f32 + 0.5 - hh, hw, hh, r);
            let outer = cov(d);
            let inner = cov(d + 1.);
            match (fill, line) {
                (Some(f), Some(l)) => {
                    let (cf, cl) = (rgb(f), rgb(l));
                    // Stroke ring over the fill, fill inside the ring.
                    let ring = outer - inner;
                    let a = outer;
                    if a > 0. {
                        let mixc = [0, 1, 2].map(|k| (cl[k] * ring + cf[k] * inner) / a);
                        put(&mut px, x, y, mixc, a);
                    }
                }
                (None, Some(l)) => put(&mut px, x, y, rgb(l), outer - inner),
                _ => {}
            }
        }
    }
    px
}

/// Open polyline on a 24-unit grid scaled to `size` px, round caps and joins, `width` px wide.
pub fn poly(size: i32, pts: &[(i32, i32)], width: f32, color: u32) -> Px {
    let pad = 3usize;
    let side = size.max(1) as usize + pad * 2;
    let mut px = Px { w: side, h: side, data: vec![0; side * side * 4] };
    let s = size as f32 / 24.;
    let p: Vec<(f32, f32)> = pts.iter().map(|(x, y)| (*x as f32 * s + pad as f32, *y as f32 * s + pad as f32)).collect();
    let c = rgb(color);
    for y in 0..side {
        for x in 0..side {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut best = f32::MAX;
            for seg in p.windows(2) {
                let (ax, ay) = seg[0];
                let (bx, by) = seg[1];
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = dx * dx + dy * dy;
                let t = if len2 == 0. { 0. } else { (((fx - ax) * dx + (fy - ay) * dy) / len2).clamp(0., 1.) };
                best = best.min((fx - ax - t * dx).hypot(fy - ay - t * dy));
            }
            let a = cov(best - width / 2.);
            if a > 0. {
                put(&mut px, x, y, c, a);
            }
        }
    }
    px
}
/// Open polyline in pixel coordinates, `width` px wide with round caps and joins. Returns the
/// pixels and the top-left position to draw them at.
pub fn polyline(pts: &[(i32, i32)], width: f32, color: u32) -> Option<(Px, i32, i32)> {
    if pts.len() < 2 {
        return None;
    }
    let pad = (width / 2.).ceil() as i32 + 2;
    let minx = pts.iter().map(|p| p.0).min()? - pad;
    let miny = pts.iter().map(|p| p.1).min()? - pad;
    let maxx = pts.iter().map(|p| p.0).max()? + pad;
    let maxy = pts.iter().map(|p| p.1).max()? + pad;
    let (w, h) = ((maxx - minx) as usize, (maxy - miny) as usize);
    let mut px = Px { w, h, data: vec![0; w * h * 4] };
    let c = rgb(color);
    let p: Vec<(f32, f32)> = pts.iter().map(|(x, y)| ((x - minx) as f32, (y - miny) as f32)).collect();
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut best = f32::MAX;
            for seg in p.windows(2) {
                let (ax, ay) = seg[0];
                let (bx, by) = seg[1];
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = dx * dx + dy * dy;
                let t = if len2 == 0. { 0. } else { (((fx - ax) * dx + (fy - ay) * dy) / len2).clamp(0., 1.) };
                best = best.min((fx - ax - t * dx).hypot(fy - ay - t * dy));
            }
            let a = cov(best - width / 2.);
            if a > 0. {
                put(&mut px, x, y, c, a);
            }
        }
    }
    Some((px, minx, miny))
}
/// Margin `poly` adds around the glyph on every side.
pub const POLY_PAD: i32 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    fn alpha(p: &Px, x: usize, y: usize) -> u8 {
        p.data[(y * p.w + x) * 4 + 3]
    }
    /// Writes the app's actual AA pixels for review: one raw RGBA file per shape and size.
    #[test]
    fn dump_pixels() {
        let Ok(dir) = std::env::var("ZIM_DUMP") else { return };
        let ring: Vec<(i32, i32)> = (0..=16)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 16.;
                ((12. + 8. * a.cos()).round() as i32, (12. + 8. * a.sin()).round() as i32)
            })
            .collect();
        for scale in [100, 125, 150, 200] {
            let size = 24 * scale / 100;
            let shapes: [(&str, Px); 3] = [
                ("ring", poly(size, &ring, 2. * size as f32 / 24., 0xffffff)),
                ("check", poly(size, &[(5, 12), (10, 17), (19, 7)], 2. * size as f32 / 24., 0xffffff)),
                ("badge", rrect(48 * scale as usize / 100, 48 * scale as usize / 100, 24 * scale / 100, Some(0x6a3a28), Some(0xc0e8a0))),
            ];
            for (name, p) in shapes {
                let mut rgba = Vec::new();
                for q in p.data.chunks(4) {
                    // Un-premultiply BGRA -> RGBA for the image tool.
                    let a = q[3] as u32;
                    let un = |c: u8| if a == 0 { 0 } else { (c as u32 * 255 / a).min(255) as u8 };
                    rgba.extend([un(q[2]), un(q[1]), un(q[0]), q[3]]);
                }
                std::fs::write(format!("{dir}/aa-{name}-{scale}.{}x{}.rgba", p.w, p.h), rgba).unwrap();
            }
        }
    }
    #[test]
    fn circle_edge_has_partial_coverage() {
        let p = rrect(32, 32, 16, Some(0xffffff), None);
        assert_eq!(alpha(&p, 16, 16), 255);
        assert_eq!(alpha(&p, 0, 0), 0);
        // Along the rim there are pixels that are neither empty nor full.
        let partial = p.data.chunks(4).filter(|q| q[3] > 8 && q[3] < 247).count();
        assert!(partial >= 40, "{partial}");
    }
    #[test]
    fn rrect_with_stroke_has_ring_and_fill() {
        let p = rrect(40, 20, 6, Some(0x202020), Some(0xff8040));
        // Middle of the left edge is stroke colored (BGR order in data), the center is fill.
        let i = (10 * 40) * 4;
        assert!(p.data[i + 3] > 200);
        assert!(p.data[i] < p.data[i + 2] || p.data[i] > 0);
        let c = ((10 * 40) + 20) * 4;
        assert_eq!(p.data[c + 3], 255);
        assert_eq!(p.data[c], 0x20);
    }
    #[test]
    fn diagonal_line_is_anti_aliased() {
        let p = poly(24, &[(2, 2), (22, 22)], 2., 0xffffff);
        let partial = p.data.chunks(4).filter(|q| q[3] > 8 && q[3] < 247).count();
        assert!(partial >= 30, "{partial}");
        assert!(p.data.chunks(4).all(|q| q[0] == q[3] && q[1] == q[3]));
    }
    #[test]
    fn nothing_leaks_outside_the_canvas_pad() {
        let p = poly(20, &[(0, 0), (24, 0)], 2., 0xffffff);
        assert_eq!(alpha(&p, 0, 0), 0);
    }
    #[test]
    fn polyline_covers_its_endpoints_and_smooths_diagonals() {
        let (p, ox, oy) = polyline(&[(10, 10), (20, 20)], 2., 0xffffff).unwrap();
        assert!(ox < 10 && oy < 10);
        let mid = ((15 - oy) as usize * p.w + (15 - ox) as usize) * 4 + 3;
        assert_eq!(p.data[mid], 255);
        let partial = p.data.chunks(4).filter(|q| q[3] > 8 && q[3] < 247).count();
        assert!(partial >= 10, "{partial}");
        assert!(polyline(&[(1, 1)], 2., 0).is_none());
    }
}
