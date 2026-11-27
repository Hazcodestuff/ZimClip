//! Pure geometry for the library column and the bottom footer band. library_view::layout and
//! every row-capacity consumer call these functions, so what is drawn, counted and paged cannot
//! disagree. No window calls here.

/// Space reserved at the bottom of the client area for the status row, the Recover/Restart/
/// Diagnostics buttons and the footer text. No library content may enter it.
pub const FOOTER_BAND: i32 = 88;
/// Distances from the client bottom (top edge of each footer item).
pub const RESULT_Y: i32 = 82;
pub const BUTTON_Y: i32 = 60;
pub const FOOTER_Y: i32 = 28;
/// The library has five row controls (ids 100 to 104).
pub const MAX_ROWS: usize = 5;
/// Action row (Open/Select/Refresh/Tags) height and its gap above the footer band's content.
pub const ACTION_H: i32 = 32;
/// Large thumbnail cards: a 16:9 picture plus the title and detail lines.
pub const GRID_GAP: i32 = 10;
/// Space under the picture for the title and the size/date line.
pub const CARD_TEXT_H: i32 = 62;
/// One-column Large card: picture left (128x72), text right.
pub const WIDE_CARD_H: i32 = 88;
/// A card at least twice as wide as tall is laid out as the wide one-column card.
pub fn is_wide_card(w: i32, h: i32) -> bool {
    w >= h * 2
}
/// (columns, card width, card height) for a library column `w` wide: three, two or one card
/// across, whichever fits. Height follows the 16:9 picture.
pub fn grid_dims(w: i32) -> (i32, i32, i32) {
    let avail = w - 14;
    let cols = if avail >= 3 * 220 + 2 * GRID_GAP { 3 } else if avail >= 2 * 200 + GRID_GAP { 2 } else { 1 };
    if cols == 1 {
        // A narrow column: a wide card with the 16:9 picture on the left (three fit a normal page).
        return (1, avail, WIDE_CARD_H);
    }
    let cw = ((avail - (cols - 1) * GRID_GAP) / cols).clamp(160, 260);
    (cols, cw, (cw - 16) * 9 / 16 + 8 + CARD_TEXT_H)
}
/// Clear space between the last row and the action row.
pub const ROW_ACTION_GAP: i32 = 4;

/// Below this client height Legacy Stacked cannot show Home and the library side by side, so it
/// pages like Compact. At or above it both stay visible, as before.
pub const LEGACY_STACKED_SIDE_BY_SIDE_MIN_H: i32 = 614;
/// Card panels end above the status row, never over the footer band.
pub fn clamp_cards(boxes: &mut [(i32, i32, i32, i32)], client_h: i32) {
    for b in boxes.iter_mut() {
        b.3 = b.3.min(client_h - RESULT_Y).max(b.1 + 40);
    }
}
pub fn content_bottom(client_h: i32) -> i32 {
    client_h - FOOTER_BAND
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibGeom {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    /// Bottom edge of the library card, never below the footer band.
    pub end: i32,
    /// First row top.
    pub top: i32,
    pub row_h: i32,
    pub gap: i32,
    pub grid: bool,
    pub cols: i32,
    pub card_w: i32,
    pub card_h: i32,
    pub compact: bool,
    pub stacked: bool,
}

/// Library card geometry. `gmode` is the layout mode, or 5/6 for the roomy Compact/Stacked
/// Clips page. `mode` is the real layout mode. Normal-height numbers are the ones the layout
/// used before; only `end` is now limited by the footer band.
pub fn lib_geom(gmode: usize, mode: usize, large: bool, polished: bool, cpage1: bool, client_w: i32, client_h: i32) -> LibGeom {
    let fit = |fallback: i32, used: i32| if polished { (client_w - used).max(300) } else { fallback };
    let (x, y, w, end) = match gmode {
        5 => (34, 104, 536, client_h - 110),
        6 => (34, 104, (client_w - 68).max(500), client_h - 110),
        1 => (34, 384, 536, 578),
        2 => (414, 104, fit(786, 448), 590),
        3 => (34, 364, fit(896, 58), 610),
        4 => { let (lx, lw) = crate::nav::library_col(client_w); (lx, 104, lw, 610) }
        _ => (508, 104, fit(422, 542), 610),
    };
    let end = end.min(content_bottom(client_h));
    let compact = gmode == 1;
    let stacked = mode == 3;
    let top = if compact { y + 78 } else if stacked { y + 92 } else { y + if large { 132 } else { 138 } };
    // The card grid needs one full card row above the action row. On a shorter client the
    // large cards fall back to the list rows, which can always show at least one row.
    let (cols, card_w, card_h) = grid_dims(w);
    // Large is a thumbnail card wherever one card fits above the action row; Compact stays a slim list.
    let grid = gmode != 1 && large && end - ACTION_H - top >= card_h;
    let (row_h, gap) = if grid {
        (card_h, GRID_GAP)
    } else if compact {
        (24, 2)
    } else if stacked {
        let room = (end - 42 - top).max(180);
        let rh = if cpage1 { (room / 5 - 6).clamp(36, 64) } else { 34 };
        (rh, 6)
    } else if large || (mode == 2 || gmode == 6) && large {
        (62, 6)
    } else {
        (48, 8)
    };
    LibGeom { x, y, w, end, top, row_h, gap, grid, cols, card_w, card_h, compact, stacked }
}

/// How many rows fit above the action row (end - ACTION_H), at least one, at most five.
/// The row size stays fixed; only the count changes, so click targets stay readable.
pub fn rows_fit(g: &LibGeom) -> usize {
    let room = g.end - ACTION_H - ROW_ACTION_GAP - g.top;
    let n = if g.grid {
        // Rows of `cols` cards.
        ((room + GRID_GAP) / (g.card_h + GRID_GAP)).max(1) as usize * g.cols as usize
    } else {
        ((room + g.gap) / (g.row_h + g.gap)).max(1) as usize
    };
    n.min(MAX_ROWS)
}

/// Rect (x, y, w, h) of row `i` for this geometry.
pub fn row_rect(g: &LibGeom, i: usize) -> (i32, i32, i32, i32) {
    if g.grid {
        let c = g.cols.max(1) as usize;
        (g.x + (i % c) as i32 * (g.card_w + GRID_GAP), g.top + (i / c) as i32 * (g.card_h + GRID_GAP), g.card_w, g.card_h)
    } else {
        (g.x, g.top + i as i32 * (g.row_h + g.gap), g.w - 14, g.row_h)
    }
}

/// Trash button rect (left, top, right, bottom) inside a card of size w x h. One function for
/// drawing and click handling, so the hit area is exactly the drawn icon.
pub fn trash_rect(w: i32, h: i32, grid: bool, dense: bool) -> (i32, i32, i32, i32) {
    let s = if dense { 20 } else { 26 };
    if grid {
        (w - 8 - s, h - 10 - s, w - 8, h - 10)
    } else {
        (w - 10 - s, (h - s) / 2, w - 10, (h - s) / 2 + s)
    }
}

/// Source rectangle (x, y, w, h) that fills a dw x dh frame from a bw x bh picture without
/// stretching it: the picture is cropped around its centre.
pub fn cover_crop(bw: i32, bh: i32, dw: i32, dh: i32) -> (i32, i32, i32, i32) {
    if bw <= 0 || bh <= 0 || dw <= 0 || dh <= 0 {
        return (0, 0, bw.max(1), bh.max(1));
    }
    if (bw as i64) * (dh as i64) > (bh as i64) * (dw as i64) {
        let sw = ((bh as i64) * (dw as i64) / dh as i64).max(1) as i32;
        ((bw - sw) / 2, 0, sw, bh)
    } else {
        let sh = ((bw as i64) * (dh as i64) / dw as i64).max(1) as i32;
        (0, (bh - sh) / 2, bw, sh)
    }
}

/// Favourite star rect: just left of the trash button, same size and row.
pub fn star_rect(w: i32, h: i32, grid: bool, dense: bool) -> (i32, i32, i32, i32) {
    let (l, t, r, b) = trash_rect(w, h, grid, dense);
    let s = r - l;
    (l - s - 4, t, l - 4, b)
}

/// Padding above the first and below the last item of an app-drawn popup menu.
pub const MENU_PAD: i32 = 6;
/// Item spans (top, bottom) stacked under the top padding, and the total window height.
pub fn menu_stack(heights: &[i32]) -> (Vec<(i32, i32)>, i32) {
    let mut y = MENU_PAD;
    let mut out = Vec::new();
    for h in heights {
        out.push((y, y + h));
        y += h;
    }
    (out, y + MENU_PAD)
}

/// Where a menu of size w x h goes for a requested top-left corner, kept inside the work area.
pub fn menu_place(x: i32, y: i32, w: i32, h: i32, work: (i32, i32, i32, i32)) -> (i32, i32) {
    let (l, t, r, b) = work;
    let nx = if x + w > r { (r - w).max(l) } else { x.max(l) };
    let ny = if y + h > b { (y - h).max(t).min((b - h).max(t)) } else { y.max(t) };
    (nx, ny)
}

/// Submenu corner: to the right of its parent (overlapping the border a little), or to the left
/// when there is no room on the right.
pub fn menu_submenu_x(parent_left: i32, parent_right: i32, w: i32, work_right: i32) -> i32 {
    if parent_right - 6 + w <= work_right {
        parent_right - 6
    } else {
        parent_left - w + 6
    }
}


/// Where the mouse wheel scrolls the library: the list area down to the action row.
pub fn list_bottom(g: &LibGeom) -> i32 {
    g.end - ACTION_H - ROW_ACTION_GAP
}

#[cfg(test)]
mod tests {
    use super::*;
    const LEGACY_STACKED_MIN_H: i32 = LEGACY_STACKED_SIDE_BY_SIDE_MIN_H;
    #[test]
    fn legacy_stacked_short_limit_is_real() {
        // Below the limit one row cannot fit: stage B must page or scroll it.
        let g = lib_geom(3, 3, false, false, false, 954, LEGACY_STACKED_MIN_H - 8);
        let (_, y, _, rh) = row_rect(&g, 0);
        assert!(y + rh > list_bottom(&g));
        let g = lib_geom(3, 3, false, false, false, 954, LEGACY_STACKED_MIN_H);
        let (_, y, _, rh) = row_rect(&g, 0);
        assert!(y + rh <= list_bottom(&g));
    }
    /// Every library state: (gmode, mode, polished, cpage1, widths).
    fn states() -> Vec<(usize, usize, bool, bool, i32)> {
        let mut v = Vec::new();
        for polished in [false, true] {
            // Balanced, Wide, Stacked (page-less), Sidebar (Polished only), Clips pages of Compact/Stacked.
            v.push((0, 0, polished, false, 970 - 16));
            v.push((2, 2, polished, false, 1240 - 16));
            if !polished { v.push((3, 3, false, false, 970 - 16)); v.push((6, 3, false, true, 970 - 16)); }
            if polished { v.push((4, 4, true, false, 1120 - 16)); }
            v.push((5, 1, polished, true, 610 - 16));
            if polished { v.push((6, 3, true, true, 970 - 16)); }
        }
        v
    }
    #[test]
    fn rows_fit_above_action_row_and_footer_band_at_every_height() {
        for (gmode, mode, polished, cpage1, w) in states() {
            for large in [false, true] {
                for h in (480..=760).step_by(8) {
                    // Legacy Stacked is not paged (stage B): its library starts below Home at
                    // y=364, so it only fits from this client height. Recorded, not hidden.
                    if gmode == 3 && !polished && h < LEGACY_STACKED_MIN_H { continue; }
                    let g = lib_geom(gmode, mode, large, polished, cpage1, w, h);
                    let n = rows_fit(&g);
                    assert!((1..=MAX_ROWS).contains(&n), "{gmode} {large} {h}");
                    // Card never enters the footer band; the action row is inside the card.
                    assert!(g.end <= content_bottom(h), "end {gmode} {large} {h}");
                    for i in 0..n {
                        let (_, y, _, rh) = row_rect(&g, i);
                        assert!(y + rh <= list_bottom(&g) + if g.grid { 8 } else { 0 }, "row {i} of {n} gmode {gmode} large {large} h {h}: {} > {}", y + rh, list_bottom(&g));
                        assert!(rh >= 24, "hit target");
                    }
                    // Rows do not overlap each other.
                    for i in 1..n {
                        let (ax, ay, aw, ah) = row_rect(&g, i - 1);
                        let (bx, by, bw, bh) = row_rect(&g, i);
                        let sep = ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay;
                        assert!(sep, "overlap {i} gmode {gmode} h {h}");
                    }
                    // Action row sits below the last row and above the footer band.
                    assert!(g.end <= content_bottom(h) && g.end - ACTION_H >= g.top, "action row {gmode} {h}");
                }
            }
        }
    }
    #[test]
    fn normal_heights_keep_the_established_row_counts() {
        // Client heights of the normal Legacy/Polished windows (740/750 minus the frame).
        let g = lib_geom(0, 0, false, true, false, 970 - 16, 698);
        assert_eq!(rows_fit(&g), 5);
        let g = lib_geom(0, 0, true, true, false, 970 - 16, 698);
        // Large cards: wide one-column cards in this narrow column, so fewer than the old list rows.
        assert!(g.grid && g.cols == 1);
        assert!(rows_fit(&g) >= 3, "{}", rows_fit(&g));
        let g = lib_geom(3, 3, false, false, false, 970 - 16, 698);
        assert_eq!(rows_fit(&g), 3);
    }
    #[test]
    fn fewer_rows_on_short_clients_not_smaller_rows() {
        let tall = lib_geom(0, 0, false, true, false, 954, 698);
        let short = lib_geom(0, 0, false, true, false, 954, 480);
        assert_eq!(tall.row_h, short.row_h);
        assert!(rows_fit(&short) < rows_fit(&tall));
        assert!(rows_fit(&short) >= 1);
    }
    #[test]
    fn crop_keeps_the_picture_undistorted() {
        let (_, y, w, h) = cover_crop(320, 180, 230, 129);
        assert!(y <= 1 && w == 320 && h >= 179);
        let (_, y, w, h) = cover_crop(320, 180, 300, 100);
        assert!(y > 0 && w == 320 && (w * 100 - h * 300).abs() <= 300);
        let (x, _, w, h) = cover_crop(320, 180, 100, 100);
        assert!(x > 0 && h == 180 && (w - h).abs() <= 1);
    }
    #[test]
    fn trash_button_stays_inside_every_card_shape() {
        for w in [300, 420, 536, 700, 954] {
            let (_, cw, ch) = grid_dims(w);
            let (l, t, r, b) = trash_rect(cw, ch, true, false);
            assert!(l >= 0 && t >= 0 && r <= cw && b <= ch && r - l >= 20, "grid {w}");
        }
        for (w, h, dense) in [(400, 62, false), (400, 48, false), (400, 24, true), (400, 34, true)] {
            let (l, t, r, b) = trash_rect(w, h, false, dense);
            assert!(l >= 0 && t >= 0 && r <= w && b <= h && r - l >= 20 && b - t >= 20, "{w}x{h}");
        }
    }
    #[test]
    fn star_and_trash_sit_inside_the_card_and_never_overlap() {
        let mut shapes = vec![(400, 62, false, false), (400, 48, false, false), (400, 24, false, true), (400, 34, false, true)];
        for w in [300, 420, 536, 700, 954] {
            let (_, cw, ch) = grid_dims(w);
            shapes.push((cw, ch, true, false));
        }
        for (w, h, grid, dense) in shapes {
            let (sl, st, sr, sb) = star_rect(w, h, grid, dense);
            let (tl, tt, tr, tb) = trash_rect(w, h, grid, dense);
            assert!(sl >= 0 && st >= 0 && sr <= w && sb <= h, "star in card {w}x{h}");
            assert!(sr <= tl || tr <= sl || sb <= tt || tb <= st, "disjoint {w}x{h}");
        }
    }
    #[test]
    fn large_cards_are_sixteen_by_nine_pictures_that_fit_their_column() {
        for w in [300, 408, 500, 536, 700, 782, 896, 954] {
            let (cols, cw, ch) = grid_dims(w);
            assert!((1..=3).contains(&cols));
            assert!(cols * cw + (cols - 1) * GRID_GAP <= w - 14 || cw == 160, "fits {w}");
            if cols > 1 {
                let pic_h = (cw - 16) * 9 / 16;
                assert!(8 + pic_h + CARD_TEXT_H <= ch);
                assert!(!is_wide_card(cw, ch));
            } else {
                assert!(is_wide_card(cw, ch));
            }
        }
        assert_eq!(grid_dims(896).0, 3);
        assert_eq!(grid_dims(536).0, 2);
        assert_eq!(grid_dims(408).0, 1);
    }
    #[test]
    fn large_is_a_card_grid_wherever_a_card_fits_and_compact_never_is() {
        for gmode in [0usize, 2, 3, 4, 5, 6] {
            let g = lib_geom(gmode, if gmode == 6 { 3 } else { gmode.min(3) }, true, true, gmode >= 5, 954, 698);
            // Stacked shows Home above the library: at 698 px there is no room for a card row, so it stays a list.
            assert_eq!(g.grid, gmode != 3, "large gmode {gmode}");
            let small = lib_geom(gmode, gmode.min(3), false, true, gmode >= 5, 954, 698);
            assert!(!small.grid, "compact gmode {gmode}");
        }
        assert!(!lib_geom(1, 0, true, true, false, 954, 698).grid);
    }
    #[test]
    fn items_stack_with_padding_and_the_height_adds_up() {
        let (spans, total) = menu_stack(&[38, 12, 38]);
        assert_eq!(spans, vec![(MENU_PAD, MENU_PAD + 38), (MENU_PAD + 38, MENU_PAD + 50), (MENU_PAD + 50, MENU_PAD + 88)]);
        assert_eq!(total, MENU_PAD + 88 + MENU_PAD);
    }
    #[test]
    fn menus_stay_inside_the_work_area() {
        let work = (0, 0, 1000, 700);
        assert_eq!(menu_place(100, 100, 300, 200, work), (100, 100));
        // Off the right edge: moved left. Off the bottom: opens upward.
        assert_eq!(menu_place(900, 100, 300, 200, work), (700, 100));
        assert_eq!(menu_place(100, 600, 300, 200, work), (100, 400));
        // Taller than the space above and below: pinned to the top.
        assert_eq!(menu_place(10, 100, 100, 650, work).1, 0);
    }
    #[test]
    fn submenus_open_right_or_flip_left() {
        assert_eq!(menu_submenu_x(100, 400, 200, 1000), 394);
        assert_eq!(menu_submenu_x(700, 1000, 200, 1000), 506);
    }
    #[test]
    fn clamped_cards_stay_above_the_status_row() {
        let mut b = vec![(16, 88, 474, 622), (490, 88, 948, 622)];
        clamp_cards(&mut b, 528);
        assert!(b.iter().all(|c| c.3 == 528 - RESULT_Y));
        let mut n = vec![(16, 88, 474, 622)];
        clamp_cards(&mut n, 701);
        assert_eq!(n[0].3, 619);
    }
    #[test]
    fn footer_band_is_the_one_constant() {
        assert!(RESULT_Y < FOOTER_BAND && BUTTON_Y < RESULT_Y && FOOTER_Y < BUTTON_Y);
        assert_eq!(content_bottom(700), 700 - FOOTER_BAND);
    }
}

// ---- Home column (Balanced and Wide, non-expanded) ----
pub type R = (i32, i32, i32, i32);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeRects {
    /// 0 = established layout, 1 = optional hint dropped, 2 = System/Mic side by side and the
    /// sources heading and note dropped. The buffer bar is kept in every tier.
    pub tier: u8,
    pub hint: Option<R>,
    pub encoder: R,
    pub buffer: R,
    pub save: R,
    pub sources: Option<R>,
    pub sys: R,
    pub mic: R,
    pub voice: R,
    pub note: Option<R>,
    pub pause: R,
    pub record: R,
    pub details: R,
}
impl HomeRects {
    pub fn rects(&self) -> Vec<R> {
        let mut v = vec![self.encoder, self.buffer, self.save, self.sys, self.mic, self.voice, self.pause, self.record, self.details];
        v.extend(self.hint);
        v.extend(self.sources);
        v.extend(self.note);
        v
    }
    pub fn bottom(&self) -> i32 {
        self.rects().iter().map(|r| r.1 + r.3).max().unwrap_or(0)
    }
}
fn tier0(wide: bool, x: i32, w: i32) -> HomeRects {
    let half = (w - 128) / 2;
    if wide {
        HomeRects {
            tier: 0, hint: Some((x, 138, w, 56)), encoder: (x, 196, w, 24), buffer: (x, 222, w, 44), save: (x, 270, w, 48),
            sources: Some((x, 328, w, 20)), sys: (x, 354, w, 46), mic: (x, 406, w, 46), voice: (x, 458, w, 46),
            note: Some((x, 512, w, 34)), pause: (x, 552, 100, 36), record: (x + 108, 552, 116, 36), details: (x + 232, 552, 96, 36),
        }
    } else {
        HomeRects {
            tier: 0, hint: Some((x, 140, w, 56)), encoder: (x, 201, w, 24), buffer: (x, 230, w, 44), save: (x, 282, w, 52),
            sources: Some((x, 344, w, 20)), sys: (x, 370, w, 46), mic: (x, 424, w, 46), voice: (x, 478, w, 46),
            note: Some((x, 534, w, 34)), pause: (x, 576, half, 36), record: (x + half + 8, 576, half, 36), details: (x + w - 104, 576, 104, 36),
        }
    }
}
fn tier1(wide: bool, x: i32, w: i32) -> HomeRects {
    let t = tier0(wide, x, w);
    HomeRects {
        tier: 1, hint: None, encoder: (x, 134, w, 24), buffer: (x, 162, w, 44), save: (x, 214, w, 52),
        sources: Some((x, 274, w, 20)), sys: (x, 298, w, 46), mic: (x, 350, w, 46), voice: (x, 402, w, 46),
        note: Some((x, 454, w, 34)), pause: (t.pause.0, 496, t.pause.2, 36), record: (t.record.0, 496, t.record.2, 36), details: (t.details.0, 496, t.details.2, 36),
    }
}
fn tier2(x: i32, w: i32) -> HomeRects {
    let pair = (w - 8) / 2;
    let third = (w - 16) / 3;
    HomeRects {
        tier: 2, hint: None, encoder: (x, 132, w, 24), buffer: (x, 160, w, 36), save: (x, 202, w, 44),
        sources: None, sys: (x, 252, pair, 40), mic: (x + pair + 8, 252, pair, 40), voice: (x, 298, w, 36),
        note: None, pause: (x, 340, third, 36), record: (x + third + 8, 340, third, 36), details: (x + 2 * (third + 8), 340, third, 36),
    }
}
/// Home column geometry for the client's content height. The first tier that fits wins, so the
/// result depends only on `content_bottom`, and growing the window restores every control.
/// `wide` selects the Wide geometry; otherwise Balanced.
pub fn home_rects(wide: bool, x: i32, w: i32, content_bottom: i32) -> HomeRects {
    let t0 = tier0(wide, x, w);
    if t0.bottom() <= content_bottom { return t0; }
    let t1 = tier1(wide, x, w);
    if t1.bottom() <= content_bottom { return t1; }
    tier2(x, w)
}

#[cfg(test)]
mod home_tests {
    use super::*;
    fn overlap(a: R, b: R) -> bool {
        !(a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1)
    }
    #[test]
    fn tier_zero_is_the_established_layout() {
        let b = tier0(false, 34, 422);
        assert_eq!((b.save, b.voice, b.pause, b.bottom()), ((34, 282, 422, 52), (34, 478, 422, 46), (34, 576, 147, 36), 612));
        let w = tier0(true, 34, 328);
        assert_eq!((w.save, w.voice, w.bottom()), ((34, 270, 328, 48), (34, 458, 328, 46), 588));
    }
    #[test]
    fn every_tier_fits_without_overlap_down_to_480() {
        for (wide, x, w) in [(false, 34, 422), (true, 34, 328)] {
            let mut last_tier = 0;
            for cb in (392..=700).rev() {
                let h = home_rects(wide, x, w, cb);
                assert!(h.tier >= last_tier, "tier never gets lower as the window shrinks");
                last_tier = h.tier;
                assert!(h.bottom() <= cb, "wide {wide} cb {cb} tier {}: bottom {}", h.tier, h.bottom());
                let r = h.rects();
                for i in 0..r.len() {
                    assert!(r[i].1 >= 104 + 24 || r[i].1 >= 132, "below heading row");
                    for j in i + 1..r.len() {
                        assert!(!overlap(r[i], r[j]), "overlap {:?} {:?} tier {}", r[i], r[j], h.tier);
                    }
                }
                // Hit targets and the kept controls.
                for b in [h.save, h.sys, h.mic, h.voice, h.pause, h.record, h.details] { assert!(b.3 >= 36 || b == h.save && b.3 >= 44 || b.3 >= 40, "{b:?}"); assert!(b.2 >= 90, "{b:?}"); }
                assert!(h.buffer.3 >= 36 && h.encoder.3 >= 24, "buffer bar and status stay");
            }
        }
    }
    #[test]
    fn measured_and_clamped_sizes_pick_the_expected_tiers() {
        // 528 client (measured short), 689 (728 work area minus frame), 701 (740 window).
        assert_eq!(home_rects(false, 34, 422, content_bottom(528)).tier, 2);
        assert_eq!(home_rects(false, 34, 422, content_bottom(689)).tier, 1);
        assert_eq!(home_rects(false, 34, 422, content_bottom(701)).tier, 0);
        assert_eq!(home_rects(true, 34, 328, content_bottom(681)).tier, 0);
        assert_eq!(home_rects(true, 34, 328, content_bottom(480)).tier, 2);
    }
}

// ---- Home column, Details expanded (Balanced and Wide) ----
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpandedRects {
    pub tier: u8,
    pub encoder: R,
    pub buffer: R,
    pub save: R,
    /// The details text block and the audio text block.
    pub details_text: R,
    pub audio_text: R,
    pub pause: R,
    pub record: R,
    /// The Details toggle button (id 40).
    pub details_button: R,
}
impl ExpandedRects {
    pub fn rects(&self) -> [R; 8] {
        [self.encoder, self.buffer, self.save, self.details_text, self.audio_text, self.pause, self.record, self.details_button]
    }
    pub fn bottom(&self) -> i32 {
        self.rects().iter().map(|r| r.1 + r.3).max().unwrap_or(0)
    }
}
/// Lowest client height at which the expanded Home column fits without overlap. Below it the
/// Details text blocks would have to shrink to unreadable heights, so this is a declared floor.
pub const EXPANDED_FLOOR_H: i32 = 518;
/// Expanded Home buttons for tiers 0/1: the Details button reads "Hide details", which needs
/// 128 px at 14 px semibold; Pause and Record share the rest. Returns (pause, record, details) x/w.
pub fn expanded_buttons(wide: bool, x: i32, w: i32) -> [(i32, i32); 3] {
    if wide {
        // The Wide column is only 328 px: the established 100/116/96 split stays (not pixel
        // covered for "Hide details"; tier 2 uses thirds).
        return [(x, 100), (x + 108, 116), (x + 232, 96)];
    }
    let details_w = 128;
    let rest = w - details_w - 16;
    let (p, r) = (rest / 2, rest - rest / 2);
    [(x, p), (x + p + 8, r), (x + p + r + 16, details_w)]
}
/// Details button label for its width: "Hide details" needs 128 px at 14 px semibold.
pub fn details_label(open: bool, width: i32) -> &'static str {
    if !open { "Details" } else if width >= 128 { "Hide details" } else { "Hide" }
}
fn exp0(wide: bool, h: HomeRects) -> ExpandedRects {
    let b = expanded_buttons(wide, h.encoder.0, h.encoder.2);
    let (pause, record, details) = (
        (b[0].0, h.pause.1, b[0].1, h.pause.3),
        (b[1].0, h.record.1, b[1].1, h.record.3),
        (b[2].0, h.details.1, b[2].1, h.details.3),
    );
    let h = HomeRects { pause, record, details, ..h };
    ExpandedRects { tier: 0, encoder: h.encoder, buffer: h.buffer, save: h.save, details_text: (h.encoder.0, 358, h.encoder.2, 78), audio_text: (h.encoder.0, 442, h.encoder.2, 100), pause: h.pause, record: h.record, details_button: h.details }
}
pub fn expanded_rects(wide: bool, x: i32, w: i32, content_bottom: i32) -> ExpandedRects {
    let t0 = exp0(wide, tier0(wide, x, w));
    if t0.bottom() <= content_bottom { return t0; }
    let base = tier0(wide, x, w);
    let b = expanded_buttons(wide, x, w);
    let (pause, record, details) = (
        (b[0].0, base.pause.1, b[0].1, base.pause.3),
        (b[1].0, base.record.1, b[1].1, base.record.3),
        (b[2].0, base.details.1, b[2].1, base.details.3),
    );
    let t1 = ExpandedRects {
        tier: 1, encoder: (x, 134, w, 24), buffer: (x, 162, w, 44), save: (x, 214, w, 52),
        details_text: (x, 272, w, 78), audio_text: (x, 356, w, 100),
        pause: (pause.0, 464, pause.2, 36), record: (record.0, 464, record.2, 36), details_button: (details.0, 464, details.2, 36),
    };
    if t1.bottom() <= content_bottom { return t1; }
    let third = (w - 16) / 3;
    ExpandedRects {
        tier: 2, encoder: (x, 132, w, 24), buffer: (x, 160, w, 36), save: (x, 202, w, 44),
        details_text: (x, 252, w, 60), audio_text: (x, 318, w, 70),
        pause: (x, 394, third, 36), record: (x + third + 8, 394, third, 36), details_button: (x + 2 * (third + 8), 394, third, 36),
    }
}

#[cfg(test)]
mod expanded_tests {
    use super::*;
    #[test]
    fn details_label_never_exceeds_its_button() {
        assert_eq!(details_label(true, 128), "Hide details");
        assert_eq!(details_label(true, 127), "Hide");
        assert_eq!(details_label(false, 60), "Details");
        for (wide, x, w) in [(false, 34, 422), (true, 34, 328)] {
            for client in 518..=760 {
                let e = expanded_rects(wide, x, w, content_bottom(client));
                let l = details_label(true, e.details_button.2);
                assert!(est_text_w(l.len() as i32, 14) + 16 <= e.details_button.2, "{l} {wide} {client}");
            }
        }
    }
    #[test]
    fn expanded_details_button_fits_hide_details_and_row_stays_inside() {
        for (wide, x, w) in [(false, 34, 422), (true, 34, 328), (false, 196, 520)] {
            let b = expanded_buttons(wide, x, w);
            if !wide { assert!(b[2].1 >= 128); }
            assert!(b[0].1 >= 90 && b[1].1 >= 90, "pause/record keep a readable width");
            assert_eq!(b[2].0 + b[2].1, x + w);
            assert!(b[0].0 + b[0].1 + 8 <= b[1].0 && b[1].0 + b[1].1 + 8 <= b[2].0);
        }
    }
    fn overlap(a: R, b: R) -> bool {
        !(a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1)
    }
    #[test]
    fn tier_zero_matches_the_established_expanded_layout() {
        let e = expanded_rects(false, 34, 422, 700);
        assert_eq!((e.tier, e.details_text, e.audio_text, e.bottom()), (0, (34, 358, 422, 78), (34, 442, 422, 100), 612));
    }
    #[test]
    fn expanded_fits_without_overlap_from_the_floor_up() {
        for (wide, x, w) in [(false, 34, 422), (true, 34, 328)] {
            let mut last = 0;
            for client in (EXPANDED_FLOOR_H..=760).rev() {
                let e = expanded_rects(wide, x, w, content_bottom(client));
                assert!(e.tier >= last);
                last = e.tier;
                assert!(e.bottom() <= content_bottom(client), "wide {wide} client {client} tier {}", e.tier);
                let r = e.rects();
                for i in 0..r.len() {
                    for j in i + 1..r.len() {
                        assert!(!overlap(r[i], r[j]), "overlap {:?} {:?} tier {} client {client}", r[i], r[j], e.tier);
                    }
                }
                assert!(e.details_text.3 >= 60 && e.audio_text.3 >= 70, "text blocks keep their lines");
                assert!(e.pause.3 >= 36 && e.buffer.3 >= 36);
            }
        }
    }
    #[test]
    fn below_the_floor_is_not_claimed() {
        // At 480 the expanded column does not fit: the floor is declared, not hidden.
        let e = expanded_rects(false, 34, 422, content_bottom(EXPANDED_FLOOR_H - 38));
        assert!(e.bottom() > content_bottom(EXPANDED_FLOOR_H - 38));
    }
}

// ---- Sidebar Home column ----
/// Source row (System audio / Microphone) text column. Wide rows keep the icon well, name,
/// description, meter and switch. Below SOURCE_NARROW_W the well, description and meter are
/// dropped so the name gets the width. Returns (text_left_offset, right_margin, font_px).
pub const SOURCE_NARROW_W: i32 = 260;
pub fn source_text_cols(row_w: i32) -> (i32, i32, i32) {
    if row_w >= SOURCE_NARROW_W { (52, 100, 14) } else { (10, 52, 13) }
}
/// Conservative width estimate of a semibold label (0.58 em per char).
pub fn est_text_w(chars: i32, font_px: i32) -> i32 {
    (chars * font_px * 58 + 99) / 100
}
#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn source_names_fit_at_every_pair_width() {
        for w in 160..=400 {
            let (l, r, f) = source_text_cols(w);
            let avail = w - l - r;
            assert!(avail >= est_text_w("System audio".len() as i32, f), "w={w} avail={avail}");
        }
    }
}
/// Sidebar Home split. When the single column does not fit above content_bottom even after the
/// hint shift, Home becomes two sub-pages. Capture = encoder, the non-Sources cards in saved
/// order, then the Pause/Record/Details row. Sources = the Sources card. Button 115 switches.
pub const SPLIT_ENCODER_Y: i32 = 134;
pub const SPLIT_FLOW_TOP: i32 = 166;
pub const SPLIT_ROW_H: i32 = 36;
pub const SPLIT_SOURCES_H: i32 = 224;
pub fn sidebar_needs_split(content_bottom: i32) -> bool {
    SIDEBAR_HOME_BOTTOM + sidebar_shift(content_bottom) > content_bottom
}
/// y of the action row (Capture) and of the 115 button on the Sources page: bottom edge at content_bottom.
pub fn split_row_y(content_bottom: i32) -> i32 { content_bottom - SPLIT_ROW_H }
/// y of the 115 button on the Capture page (above the action row).
pub fn split_switch_y_capture(content_bottom: i32) -> i32 { split_row_y(content_bottom) - SPLIT_ROW_H - 8 }
/// Sources card rows relative to its top: (heading y, [row y;3] with row height, note y, note h).
/// Full: the established 46 px rows at 54 px pitch plus the note. Tight (content_bottom below
/// SPLIT_TIGHT_BELOW_CB): 36 px rows (the hit-target minimum) at 44 px pitch, note dropped.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct SourcesRects { pub rows_y: [i32; 3], pub row_h: i32, pub note: Option<(i32, i32)>, pub height: i32 }
pub const SPLIT_TIGHT_BELOW_CB: i32 = 434;
pub fn sources_rects(content_bottom: i32) -> SourcesRects {
    if content_bottom >= SPLIT_TIGHT_BELOW_CB {
        SourcesRects { rows_y: [26, 80, 134], row_h: 46, note: Some((190, 34)), height: SPLIT_SOURCES_H }
    } else {
        SourcesRects { rows_y: [26, 70, 114], row_h: 36, note: None, height: 150 }
    }
}
/// Lowest content_bottom at which both split pages fit, for `capture_cards_h` of non-Sources cards.
pub fn split_min_content_bottom(capture_cards_h: i32) -> i32 {
    let capture = SPLIT_FLOW_TOP + capture_cards_h + 8 + 2 * SPLIT_ROW_H + 8;
    let sources = SPLIT_FLOW_TOP + sources_rects(0).height + 8 + SPLIT_ROW_H;
    capture.max(sources)
}
#[cfg(test)]
mod split_tests {
    use super::*;
    #[test]
    fn split_applies_only_when_single_column_does_not_fit() {
        assert!(!sidebar_needs_split(content_bottom(701)));
        assert!(!sidebar_needs_split(content_bottom(689)));
        assert!(!sidebar_needs_split(content_bottom(SIDEBAR_FLOOR_H)));
        assert!(sidebar_needs_split(content_bottom(SIDEBAR_FLOOR_H - 1)));
        assert!(sidebar_needs_split(content_bottom(528)));
    }
    #[test]
    fn both_pages_fit_from_declared_minimum_up() {
        // Declared minimum client for the split Sidebar Home: content_bottom + footer band.
        let cards_h = 44 + 8 + 52;
        // With the tight Sources variant the split Home fits from client 448.
        assert_eq!(split_min_content_bottom(cards_h) + FOOTER_BAND, 448);
        // The full variant is used only where it fits.
        assert!(SPLIT_FLOW_TOP + sources_rects(434).height + 8 + SPLIT_ROW_H <= 434);
        assert!(sources_rects(433).row_h >= 36);
        // Below 448 the pages do not fit: declared unsupported, asserted here.
        assert!(split_min_content_bottom(cards_h) > content_bottom(447));
    }
    #[test]
    fn both_pages_fit_522_up() {
        // Buffer 44 + Save 52 + one gap, the largest non-Sources set.
        let cards_h = 44 + 8 + 52;
        for client in 448..=760 {
            let cb = content_bottom(client);
            let s = sources_rects(cb);
            assert!(SPLIT_FLOW_TOP + s.height + 8 + SPLIT_ROW_H <= cb, "sources client {client}");
            if !sidebar_needs_split(cb) { continue; }
            assert!(split_min_content_bottom(cards_h) <= cb, "client {client}");
            assert!(split_switch_y_capture(cb) >= SPLIT_FLOW_TOP + cards_h, "client {client}");
        }
    }
}
/// Sidebar Home ends at y=612 (buttons row). Shorter clients move the whole column up by the
/// hint row. Below SIDEBAR_FLOOR_H it still does not fit: a declared floor.
pub const SIDEBAR_HOME_BOTTOM: i32 = 612;
pub const SIDEBAR_HINT_SHIFT: i32 = -68;
pub const SIDEBAR_FLOOR_H: i32 = SIDEBAR_HOME_BOTTOM + SIDEBAR_HINT_SHIFT + FOOTER_BAND;
pub fn sidebar_shift(content_bottom: i32) -> i32 {
    if content_bottom >= SIDEBAR_HOME_BOTTOM { 0 } else { SIDEBAR_HINT_SHIFT }
}
#[cfg(test)]
mod sidebar_tests {
    use super::*;
    #[test]
    fn sidebar_shift_depends_only_on_height() {
        assert_eq!(sidebar_shift(content_bottom(701)), 0);
        assert_eq!(sidebar_shift(content_bottom(689)), -68);
        assert!(SIDEBAR_HOME_BOTTOM + sidebar_shift(content_bottom(SIDEBAR_FLOOR_H)) <= content_bottom(SIDEBAR_FLOOR_H));
        assert_eq!(SIDEBAR_FLOOR_H, 632);
    }
}
