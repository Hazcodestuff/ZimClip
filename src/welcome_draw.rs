//! Draw lists for the first-run setup window. Pure data, no Windows calls: the Windows
//! painter runs these ops with GDI, and a Linux test dumps the same ops for review images.
//! Colors are GDI 0x00BBGGRR, as in the theme tables.

pub type Rc = (i32, i32, i32, i32);

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Rounded rectangle. radius >= half the shorter side makes a pill or circle.
    Rect { r: Rc, fill: Option<u32>, stroke: Option<u32>, radius: i32 },
    Text { r: Rc, s: String, px: i32, weight: i32, color: u32, align: Align },
    /// Open polyline on a 24-unit grid, scaled to `size` px at (x, y).
    Poly { x: i32, y: i32, size: i32, pts: Vec<(i32, i32)>, color: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Everything the drawing depends on.
#[derive(Clone, Debug)]
pub struct View {
    pub step: usize,
    pub layout: usize,
    pub theme: usize,
    pub accent: usize,
    /// bg, panel, border, text, muted for the six built-in themes plus the custom one (index 6).
    pub themes: [[u32; 5]; 7],
    pub accents: [u32; 8],
    pub mic: bool,
    pub apps: String,
    /// Position in the page order (0 = welcome).
    pub pos: usize,
    /// Chosen performance preset, when the saved width/fps match one.
    pub preset: Option<usize>,
    /// App name shown on the welcome page.
    pub name: String,
    /// 0 = ZimClip default, 1..=GLYPH_ICONS = built-in glyph, GLYPH_ICONS+1 = uploaded image.
    pub icon: usize,
    pub has_upload: bool,
    /// Short values for the summary page cards: layout, colors, audio, comms, shortcut, capture.
    pub summary: Vec<String>,
}
impl View {
    pub fn p(&self) -> [u32; 5] {
        self.themes[self.theme % 7]
    }
    pub fn accent_color(&self) -> u32 {
        self.accents[self.accent % 8]
    }
}

pub const CARD_TOP: i32 = 190;
pub const CARD_H: i32 = 124;
pub const CARD_GAP: i32 = 12;
pub const ACCENT_CY: i32 = 372;
/// Layout page: five cards in one row, then a larger preview.
pub const LAYOUT_TOP: i32 = 140;
pub const LAYOUT_H: i32 = 172;
pub const LAYOUT_GAP: i32 = 12;
pub const LAYOUT_COUNT: i32 = 5;
pub const LAYOUT_NAMES: [&str; 5] = ["Balanced", "Compact", "Wide", "Stacked", "Sidebar"];
/// Native control rectangles (x, y, w, h) that sit beside drawn text, kept here so tests can
/// check the gaps.
pub const HOTKEY_TEXT: Rc = (28, 134, 716, 178);
pub const HOTKEY_FIELD: Rc = (28, 196, 716, 240);
pub const NAME_FIELD: Rc = (28, 156, 716, 200);
/// Summary cards: two columns, three rows.
pub fn summary_card(i: usize) -> Rc {
    let (col, row) = ((i % 2) as i32, (i / 2) as i32);
    let (l, r) = crate::welcome_logic::card_span(col, 28, 688, 2, 14);
    let t = 136 + row * 88;
    (l, t, r, t + 76)
}
/// Page order. Values are page kinds: 0 layout, 1 theme, 2 audio, 3 apps, 4 hotkey+presets,
/// 5 summary, 6 welcome, 7 name and icon.
pub const ORDER: [usize; 9] = [6, 7, 0, 1, 2, 8, 3, 4, 5];
pub const PAGES: usize = 9;
pub const TITLES: [(&str, &str); 9] = [
    ("Pick a layout", "Five layouts, the same recorder."),
    ("Theme & accent", "Choose how ZimClip looks. Preview only until Done."),
    ("Audio devices", "System, Mic and Comms stay on separate tracks."),
    ("Which app do you talk on?", "Its audio is saved as a separate track."),
    ("Hotkey & performance", "One key to clip, and a capture size that fits your PC."),
    ("All set", "Done saves these choices and opens Replay."),
    ("", ""),
    ("Make it yours", "Rename the app and pick an icon."),
    ("Clips", "Where clips go, how much to keep, and a default share size."),
];
const GLYPHS: [&[&[(i32, i32)]]; 9] = [
    &[&[(3, 4), (21, 4), (21, 20), (3, 20), (3, 4)], &[(9, 4), (9, 20)]],
    &[&[(12, 3), (18, 11), (18, 16), (15, 20), (9, 20), (6, 16), (6, 11), (12, 3)]],
    &[&[(9, 4), (15, 4), (15, 12), (9, 12), (9, 4)], &[(6, 11), (6, 14), (9, 17), (15, 17), (18, 14), (18, 11)], &[(12, 17), (12, 21)]],
    &[&[(5, 14), (5, 10), (8, 5), (16, 5), (19, 10), (19, 14)], &[(4, 13), (8, 13), (8, 19), (4, 19), (4, 13)], &[(16, 13), (20, 13), (20, 19), (16, 19), (16, 13)]],
    &[&[(13, 2), (4, 14), (11, 14), (10, 22), (20, 9), (13, 9)]],
    &[&[(5, 12), (10, 17), (19, 7)]],
    &[&[(13, 2), (4, 14), (11, 14), (10, 22), (20, 9), (13, 9), (13, 2)]],
    &[&[(12, 3), (14, 10), (21, 12), (14, 14), (12, 21), (10, 14), (3, 12), (10, 10), (12, 3)]],
    &[&[(3, 6), (10, 6), (12, 9), (21, 9), (21, 19), (3, 19), (3, 6)]],
];
/// Number of built-in glyph icons (choice 1..=GLYPH_ICONS). Choice 0 is the ZimClip default.
pub const GLYPH_ICONS: usize = 13;
/// Outline of icon choice `i` (0 = default mark) on the 24-unit grid. Shared by the setup
/// grid and the window/tray icon renderer.
pub fn icon_strokes(i: usize) -> Vec<Vec<(i32, i32)>> {
    let ring = |r: i32| -> Vec<(i32, i32)> {
        let d = r * 7 / 10;
        vec![(12 - r, 12), (12 - d, 12 - d), (12, 12 - r), (12 + d, 12 - d), (12 + r, 12), (12 + d, 12 + d), (12, 12 + r), (12 - d, 12 + d), (12 - r, 12)]
    };
    match i {
        0 => vec![ring(9), vec![(10, 8), (16, 12), (10, 16), (10, 8)]],
        1 => vec![vec![(13, 2), (4, 14), (11, 14), (10, 22), (20, 9), (13, 9), (13, 2)]],
        2 => vec![vec![(8, 5), (19, 12), (8, 19), (8, 5)]],
        3 => vec![vec![(3, 6), (21, 6), (21, 18), (3, 18), (3, 6)], vec![(3, 10), (21, 10)], vec![(8, 6), (6, 10)], vec![(14, 6), (12, 10)]],
        4 => vec![ring(8), ring(4)],
        5 => vec![vec![(5, 8), (19, 8), (22, 15), (19, 18), (15, 15), (9, 15), (5, 18), (2, 15), (5, 8)], vec![(7, 10), (7, 13)], vec![(5, 11), (9, 11)]],
        6 => vec![vec![(12, 2), (18, 10), (18, 16), (14, 21), (10, 21), (6, 16), (6, 11), (9, 13), (12, 2)]],
        7 => vec![vec![(3, 8), (8, 8), (10, 5), (14, 5), (16, 8), (21, 8), (21, 19), (3, 19), (3, 8)], ring(3)],
        8 => vec![vec![(6, 4), (18, 4), (22, 10), (12, 21), (2, 10), (6, 4)], vec![(2, 10), (22, 10)]],
        9 => vec![vec![(12, 2), (16, 8), (16, 15), (8, 15), (8, 8), (12, 2)], vec![(8, 15), (5, 19)], vec![(16, 15), (19, 19)]],
        10 => vec![vec![(12, 2), (15, 9), (22, 9), (16, 14), (18, 21), (12, 17), (6, 21), (8, 14), (2, 9), (9, 9), (12, 2)]],
        11 => vec![vec![(7, 3), (17, 3), (17, 10), (12, 14), (7, 10), (7, 3)], vec![(12, 14), (12, 19)], vec![(8, 20), (16, 20)]],
        12 => vec![vec![(12, 20), (3, 11), (3, 7), (7, 4), (12, 8), (17, 4), (21, 7), (21, 11), (12, 20)]],
        _ => vec![vec![(12, 2), (20, 5), (20, 12), (12, 22), (4, 12), (4, 5), (12, 2)]],
    }
}
/// Icon tile `i` (0..=GLYPH_ICONS) in the 12-per-row grid; the Upload button follows it.
pub fn icon_tile(i: usize) -> Rc {
    let (col, row) = ((i % 11) as i32, (i / 11) as i32);
    let l = 28 + col * 62;
    let t = 262 + row * 62;
    (l, t, l + 52, t + 52)
}
pub fn upload_rect() -> Rc {
    let r = icon_tile(GLYPH_ICONS + 1);
    (r.0, r.1 + 4, r.0 + 150, r.3 - 4)
}
pub fn icon_at(x: i32, y: i32) -> Option<usize> {
    (0..=GLYPH_ICONS).find(|i| {
        let r = icon_tile(*i);
        x >= r.0 && x < r.2 && y >= r.1 && y < r.3
    })
}
/// Performance presets: (name, capture width, fps, note). Written to the same capture
/// width and FPS settings as Settings. Bitrate is not changed.
pub const PRESETS: [(&str, u32, u32); 3] = [("Light", 854, 30), ("Balanced", 1280, 30), ("Quality", 1920, 60)];
pub fn preset_rect(i: usize) -> Rc {
    let (l, r) = crate::welcome_logic::card_span(i as i32, 28, 688, 3, 12);
    (l, 292, r, 388)
}
pub fn preset_at(x: i32, y: i32) -> Option<usize> {
    (0..3).find(|i| {
        let r = preset_rect(*i);
        x >= r.0 && x < r.2 && y >= r.1 && y < r.3
    })
}
/// Preset whose width and fps match the saved capture settings.
pub fn preset_for(width: u32, fps: u32) -> Option<usize> {
    PRESETS.iter().position(|p| p.1 == width && p.2 == fps)
}

pub fn mix(a: u32, b: u32, pct: u32) -> u32 {
    let p = pct.min(100);
    let mut c = 0;
    for shift in [0, 8, 16] {
        c |= (((a >> shift & 255) * (100 - p) + (b >> shift & 255) * p) / 100) << shift;
    }
    c
}
/// Black or white, whichever reads better on `c`.
pub fn ink(c: u32) -> u32 {
    let l = |s: u32| (c >> s & 255) as f64;
    if 0.299 * l(0) + 0.587 * l(8) + 0.114 * l(16) > 150.0 { 0 } else { 0xffffff }
}
fn rect(r: Rc, fill: Option<u32>, stroke: Option<u32>, radius: i32) -> Op {
    Op::Rect { r, fill, stroke, radius }
}
fn text(r: Rc, s: &str, px: i32, weight: i32, color: u32, align: Align) -> Op {
    Op::Text { r, s: s.to_string(), px, weight, color, align }
}
fn glyph(ops: &mut Vec<Op>, i: usize, x: i32, y: i32, size: i32, color: u32) {
    for pts in GLYPHS[i] {
        ops.push(Op::Poly { x, y, size, pts: pts.to_vec(), color });
    }
}

/// Dot center for accent choice `i`.
pub fn accent_dot(i: i32) -> (i32, i32) {
    (28 + 19 + i * 56, ACCENT_CY)
}
pub fn custom_rect() -> Rc {
    (476, 314, 716, 346)
}
pub fn in_rect(r: Rc, x: i32, y: i32) -> bool {
    x >= r.0 && x < r.2 && y >= r.1 && y < r.3
}
pub fn accent_at(x: i32, y: i32) -> Option<usize> {
    (0..8).find(|i| {
        let (cx, cy) = accent_dot(*i);
        (x - cx) * (x - cx) + (y - cy) * (y - cy) <= 19 * 19
    }).map(|i| i as usize)
}
pub fn chip_rect(i: usize) -> Rc {
    let (col, row) = ((i % 4) as i32, (i / 4) as i32);
    let l = 28 + col * 176;
    let t = 222 + row * 68;
    (l, t, l + 160, t + 56)
}

/// Background drawing: header, progress, divider, footer count, and the page's cards.
pub fn base(v: &View) -> Vec<Op> {
    let p = v.p();
    let (bg, panel, border, tx, mu) = (p[0], p[1], p[2], p[3], p[4]);
    let ac = v.accent_color();
    let mut o = vec![rect((0, 0, 744, 600), Some(bg), None, 0)];
    let s = v.step.min(8);
    let pos = v.pos.min(PAGES - 1);
    // Progress: done = small dot, current = wide pill, next = dim dot.
    let (mut x, y) = (28, if s == 6 { 28 } else { 100 });
    for i in 0..PAGES {
        let (w, c) = if i == pos { (28, ac) } else if i < pos { (8, mix(bg, ac, 55)) } else { (8, border) };
        o.push(rect((x, y, x + w, y + 8), Some(c), None, 4));
        x += w + 8;
    }
    if s == 6 {
        welcome_page(&mut o, v);
    } else {
        o.push(rect((32, 22, 80, 70), Some(mix(panel, ac, 14)), None, 24));
        glyph(&mut o, s, 44, 34, 24, ac);
        o.push(text((96, 22, 560, 50), TITLES[s].0, 22, 700, tx, Align::Left));
        o.push(text((96, 50, 560, 72), TITLES[s].1, 13, 400, mu, Align::Left));
        o.push(rect((28, 122, 716, 123), Some(border), None, 0));
    }
    o.push(rect((28, 520, 716, 521), Some(border), None, 0));
    o.push(text((310, 534, 420, 566), &format!("{} / {}", pos + 1, PAGES), 14, 400, mu, Align::Center));
    match s {
        0 => layout_page(&mut o, v),
        1 => theme_page(&mut o, v),
        4 => preset_cards(&mut o, v),
        7 => icon_page(&mut o, v),
        8 => clips_page(&mut o, v),
        2 => audio_page(&mut o, v),
        5 => summary_page(&mut o, v),
        _ => {}
    }
    o
}

fn welcome_page(o: &mut Vec<Op>, v: &View) {
    let p = v.p();
    let ac = v.accent_color();
    o.push(rect((316, 104, 428, 216), Some(mix(p[1], ac, 22)), Some(ac), 30));
    o.push(Op::Poly { x: 346, y: 134, size: 52, pts: vec![(13, 2), (4, 14), (11, 14), (10, 22), (20, 9), (13, 9), (13, 2)], color: ac });
    o.push(text((28, 236, 716, 284), "Welcome to", 34, 700, p[3], Align::Center));
    o.push(text((28, 280, 716, 330), &v.name, 34, 700, ac, Align::Center));
    for (i, line) in [
        "Save the last moments of your game with one key.",
        "This quick setup picks your layout, colors, audio and shortcut.",
        "It takes about a minute, and you can change everything later.",
    ].iter().enumerate() {
        let t = 340 + i as i32 * 22;
        o.push(text((60, t, 684, t + 22), line, 14, 400, p[4], Align::Center));
    }
    for (i, (icon, label)) in [(1usize, "Instant clips"), (2, "Separate audio tracks"), (3, "Kept on your PC")].iter().enumerate() {
        let (l, r) = crate::welcome_logic::card_span(i as i32, 56, 632, 3, 14);
        o.push(rect((l, 424, r, 500), Some(p[1]), Some(p[2]), 16));
        let g = match icon { 1 => 1usize, 2 => 2, _ => 3 };
        let _ = g;
        let strokes = match i { 0 => icon_strokes(1), 1 => vec![vec![(9, 4), (15, 4), (15, 12), (9, 12), (9, 4)], vec![(6, 11), (6, 14), (9, 17), (15, 17), (18, 14), (18, 11)], vec![(12, 17), (12, 21)]], _ => icon_strokes(13) };
        for pts in strokes {
            o.push(Op::Poly { x: (l + r) / 2 - 10, y: 436, size: 20, pts, color: ac });
        }
        o.push(text((l, 462, r, 490), label, 14, 600, p[3], Align::Center));
    }
}

/// Labels for the setup Clips page. The folder button, length box and size chips are controls.
pub const FOLDER_BTN: Rc = (28, 152, 716, 200);
pub const SECS_EDIT: Rc = (28, 250, 148, 292);
pub const SHARE_CHIP_Y: i32 = 346;
pub const SHARE_LABELS: [&str; 6] = ["None", "8 MB", "10 MB", "20 MB", "25 MB", "50 MB"];
pub fn share_chip(i: usize) -> Rc {
    let l = 28 + i as i32 * 116;
    (l, SHARE_CHIP_Y, l + 104, SHARE_CHIP_Y + 44)
}
fn clips_page(o: &mut Vec<Op>, v: &View) {
    let p = v.p();
    o.push(text((28, 130, 400, 150), "Save folder", 15, 600, p[3], Align::Left));
    o.push(text((28, 216, 400, 240), "Clip length", 15, 600, p[3], Align::Left));
    o.push(text((164, 258, 460, 284), "seconds of replay kept (5 to 300)", 13, 400, p[4], Align::Left));
    o.push(text((28, 312, 400, 336), "Share size", 15, 600, p[3], Align::Left));
    // Single-line text ops do not wrap, so the note is split by hand into lines that fit 688 px.
    for (k, line) in [
        "Default export target, not a guaranteed file size. It only preselects the size in the clip editor.",
        "Nothing is exported or compressed automatically.",
    ]
    .iter()
    .enumerate()
    {
        o.push(text((28, 402 + k as i32 * 20, 716, 422 + k as i32 * 20), line, 12, 400, p[4], Align::Left));
    }
}
fn audio_page(o: &mut Vec<Op>, v: &View) {
    let p = v.p();
    o.push(text((28, 258, 400, 280), "Microphone input", 13, 600, p[3], Align::Left));
    o.push(text((28, 346, 500, 368), "System output device to capture", 13, 600, p[3], Align::Left));
    for (k, line) in [
        "Choices here never change Windows playback or microphone settings.",
        "A device that is unplugged stays silent and is never replaced silently.",
        "Voice chat isolation, chosen on a later page, covers all output devices.",
    ]
    .iter()
    .enumerate()
    {
        o.push(text((28, 422 + k as i32 * 18, 716, 440 + k as i32 * 18), line, 12, 400, p[4], Align::Left));
    }
}
fn icon_page(o: &mut Vec<Op>, v: &View) {
    let p = v.p();
    let ac = v.accent_color();
    o.push(text((28, 130, 300, 152), "App name", 15, 600, p[3], Align::Left));
    o.push(rect((28, 214, 716, 215), Some(p[2]), None, 0));
    o.push(text((28, 226, 300, 252), "Icon", 15, 600, p[3], Align::Left));
    for i in 0..=GLYPH_ICONS {
        let r = icon_tile(i);
        let sel = v.icon == i;
        o.push(rect(r, Some(if sel { ac } else { p[1] }), Some(if sel { ac } else { p[2] }), 14));
        for pts in icon_strokes(i) {
            o.push(Op::Poly { x: r.0 + 14, y: r.1 + 14, size: 24, pts, color: if sel { ink(ac) } else { mix(p[3], ac, 50) } });
        }
    }
    let u = upload_rect();
    let sel = v.icon == GLYPH_ICONS + 1;
    o.push(rect(u, Some(if sel { mix(p[1], ac, 14) } else { p[0] }), Some(if sel { ac } else { p[2] }), 18));
    o.push(Op::Poly { x: u.0 + 14, y: u.1 + 9, size: 16, pts: vec![(12, 15), (12, 4), (7, 9), (12, 4), (17, 9)], color: p[3] });
    o.push(text((u.0 + 36, u.1, u.2, u.3), if v.has_upload { "Your image" } else { "Upload image" }, 13, 600, p[3], Align::Left));
}

fn preset_cards(o: &mut Vec<Op>, v: &View) {
    let p = v.p();
    o.push(text((28, 258, 400, 284), "Capture size", 15, 600, p[3], Align::Left));
    let notes = ["Smallest capture size", "Matches the default", "Sharper, 60 fps"];
    for (i, (name, w, f)) in PRESETS.iter().enumerate() {
        let r = preset_rect(i);
        card_frame(o, r, v.preset == Some(i), v);
        o.push(text((r.0 + 16, r.1 + 12, r.2 - 12, r.1 + 36), name, 15, 600, p[3], Align::Left));
        o.push(text((r.0 + 16, r.1 + 40, r.2 - 12, r.1 + 62), &format!("{w} wide  /  {f} fps"), 12, 400, p[4], Align::Left));
        o.push(text((r.0 + 16, r.1 + 62, r.2 - 12, r.1 + 84), notes[i], 12, 400, p[4], Align::Left));
    }
    o.push(text((28, 404, 716, 440), "Presets set capture width and FPS only. Bitrate stays as is. Changing them later restarts capture.", 12, 400, p[4], Align::Left));
}

fn card_frame(o: &mut Vec<Op>, r: Rc, selected: bool, v: &View) {
    let p = v.p();
    let ac = v.accent_color();
    if selected {
        o.push(rect(r, Some(mix(p[1], ac, 10)), Some(ac), 14));
        o.push(rect((r.0 + 1, r.1 + 1, r.2 - 1, r.3 - 1), None, Some(ac), 13));
    } else {
        o.push(rect(r, Some(p[1]), Some(p[2]), 14));
    }
}

/// Miniature app window for layout `mode`, drawn into `area`.
pub fn mini_layout(o: &mut Vec<Op>, area: Rc, mode: usize, v: &View) {
    let p = v.p();
    let ac = v.accent_color();
    o.push(rect(area, Some(p[0]), Some(p[2]), 8));
    o.push(rect((area.0, area.1, area.2, area.1 + 8), Some(p[1]), None, 0));
    for k in 0..3 {
        o.push(rect((area.0 + 6 + k * 8, area.1 + 2, area.0 + 10 + k * 8, area.1 + 6), Some(p[4]), None, 2));
    }
    let inner = (area.0 + 6, area.1 + 14, area.2 - 6, area.3 - 6);
    for (n, f) in crate::welcome_logic::layout_blocks(mode).iter().enumerate() {
        let w = inner.2 - inner.0;
        let h = inner.3 - inner.1;
        let r = (inner.0 + w * f.0 / 1000, inner.1 + h * f.1 / 1000, inner.0 + w * f.2 / 1000, inner.1 + h * f.3 / 1000);
        o.push(rect(r, Some(if n == 0 { mix(p[1], ac, 35) } else { p[1] }), Some(if n == 0 { ac } else { p[2] }), 4));
    }
}

/// Check badge on the chosen card, drawn after the card's contents so nothing covers it.
fn card_check(o: &mut Vec<Op>, r: Rc, v: &View) {
    let ac = v.accent_color();
    o.push(rect((r.2 - 32, r.1 + 8, r.2 - 12, r.1 + 28), Some(ac), None, 10));
    o.push(Op::Poly { x: r.2 - 30, y: r.1 + 10, size: 16, pts: vec![(5, 12), (10, 17), (19, 7)], color: ink(ac) });
}

fn layout_page(o: &mut Vec<Op>, v: &View) {
    let p = v.p();
    let notes = ["Library and clip", "Capture first", "Big library", "Stacked studio", "Home and Clips"];
    for i in 0..LAYOUT_COUNT {
        let (l, r) = crate::welcome_logic::card_span(i, 28, 688, LAYOUT_COUNT, LAYOUT_GAP);
        let rc = (l, LAYOUT_TOP, r, LAYOUT_TOP + LAYOUT_H);
        card_frame(o, rc, v.layout == i as usize, v);
        mini_layout(o, (l + 12, LAYOUT_TOP + 14, r - 12, LAYOUT_TOP + 104), i as usize, v);
        o.push(text((l + 14, LAYOUT_TOP + 114, r - 8, LAYOUT_TOP + 136), LAYOUT_NAMES[i as usize], 14, 600, p[3], Align::Left));
        o.push(text((l + 14, LAYOUT_TOP + 138, r - 8, LAYOUT_TOP + 158), notes[i as usize], 12, 400, p[4], Align::Left));
        if v.layout == i as usize {
            card_check(o, rc, v);
        }
    }
    o.push(text((28, 328, 300, 348), "Preview (saved on Done)", 13, 600, p[3], Align::Left));
    mini_layout(o, (28, 354, 716, 474), v.layout, v);
}

/// Icon-led summary: six cards, each a round icon, a small label and a short value.
fn summary_page(o: &mut Vec<Op>, v: &View) {
    let p = v.p();
    let ac = v.accent_color();
    let labels = ["Layout", "Colors", "Audio", "Comms", "Shortcut", "Capture", "Clips", "Name"];
    let glyphs: [Vec<Vec<(i32, i32)>>; 8] = [
        GLYPHS[0].iter().map(|g| g.to_vec()).collect(),
        GLYPHS[1].iter().map(|g| g.to_vec()).collect(),
        GLYPHS[2].iter().map(|g| g.to_vec()).collect(),
        GLYPHS[3].iter().map(|g| g.to_vec()).collect(),
        GLYPHS[4].iter().map(|g| g.to_vec()).collect(),
        vec![vec![(3, 5), (21, 5), (21, 16), (3, 16), (3, 5)], vec![(9, 20), (15, 20)], vec![(12, 16), (12, 20)]],
        GLYPHS[8].iter().map(|g| g.to_vec()).collect(),
        GLYPHS[7].iter().map(|g| g.to_vec()).collect(),
    ];
    for i in 0..8 {
        let r = summary_card(i);
        o.push(rect(r, Some(p[1]), Some(p[2]), 14));
        let mid = (r.1 + r.3) / 2;
        o.push(rect((r.0 + 16, mid - 22, r.0 + 60, mid + 22), Some(mix(p[1], ac, 16)), None, 22));
        for pts in &glyphs[i] {
            o.push(Op::Poly { x: r.0 + 26, y: mid - 12, size: 24, pts: pts.clone(), color: ac });
        }
        o.push(text((r.0 + 76, r.1 + 10, r.2 - 12, r.1 + 32), labels[i], 12, 400, p[4], Align::Left));
        let value = v.summary.get(i).map(|s| s.as_str()).unwrap_or("");
        o.push(text((r.0 + 76, r.1 + 34, r.2 - if i == 1 { 44 } else { 12 }, r.1 + 62), value, 16, 700, p[3], Align::Left));
        if i == 1 {
            o.push(rect((r.2 - 36, mid - 10, r.2 - 16, mid + 10), Some(ac), None, 10));
        }
    }
}

fn theme_page(o: &mut Vec<Op>, v: &View) {
    let p = v.p();
    let names = ["Charcoal", "Midnight", "Plum", "Ember", "Forest", "High contrast"];
    let ac = v.accent_color();
    for i in 0..6 {
        let (l, r) = crate::welcome_logic::card_span(i, 28, 688, 6, 10);
        let rc = (l, CARD_TOP, r, CARD_TOP + CARD_H - 20);
        card_frame(o, rc, v.theme == i as usize, v);
        let t = v.themes[i as usize];
        let a = (l + 8, CARD_TOP + 8, r - 8, CARD_TOP + 66);
        o.push(rect(a, Some(t[0]), Some(t[2]), 8));
        o.push(rect((a.0 + 6, a.1 + 6, a.2 - 28, a.3 - 6), Some(t[1]), Some(t[2]), 5));
        o.push(rect((a.2 - 22, a.3 - 30, a.2 - 12, a.3 - 8), Some(ac), None, 4));
        o.push(rect((a.2 - 22, a.1 + 6, a.2 - 12, a.3 - 34), Some(t[4]), None, 4));
        o.push(text((l + 10, CARD_TOP + 72, r - 6, CARD_TOP + 98), names[i as usize], 12, 600, p[3], Align::Left));
        if v.theme == i as usize {
            card_check(o, rc, v);
        }
    }
    o.push(text((28, 322, 300, 342), "Accent color", 13, 600, p[3], Align::Left));
    for i in 0..8 {
        let (cx, cy) = accent_dot(i);
        if v.accent == i as usize {
            o.push(rect((cx - 19, cy - 19, cx + 19, cy + 19), Some(p[0]), Some(p[3]), 19));
        }
        o.push(rect((cx - 14, cy - 14, cx + 14, cy + 14), Some(v.accents[i as usize]), None, 14));
        if v.accent == i as usize {
            o.push(Op::Poly { x: cx - 8, y: cy - 8, size: 16, pts: vec![(5, 12), (10, 17), (19, 7)], color: ink(v.accents[i as usize]) });
        }
    }
    // Custom colours: opens the shared editor. Shows the custom palette and a check when it is the active theme.
    let cr = custom_rect();
    let on = v.theme == 6;
    o.push(rect(cr, Some(if on { mix(p[1], ac, 10) } else { p[1] }), Some(if on { ac } else { p[2] }), 15));
    let c = v.themes[6];
    for (k, col) in [c[0], c[1], c[3]].iter().enumerate() {
        let cx = cr.0 + 20 + k as i32 * 20;
        o.push(rect((cx - 8, cr.1 + 7, cx + 8, cr.1 + 23), Some(*col), Some(p[2]), 8));
    }
    o.push(text((cr.0 + 76, cr.1, cr.2 - 8, cr.3), "Custom colours...", 12, 600, p[3], Align::Left));
    o.push(text((28, 410, 716, 440), "Cards, dots and this window change as you click. Skip or close restores your colors.", 13, 400, p[4], Align::Left));
}

/// Voice-chat chip: round letter avatar, name, check when chosen. `r` is the chip's size at (0, 0).
pub fn chip(v: &View, i: usize, chosen: bool, w: i32, h: i32) -> Vec<Op> {
    let p = v.p();
    let ac = v.accent_color();
    let (name, _) = crate::welcome_logic::COMMON_APPS[i];
    let col = AVATAR[i % AVATAR.len()];
    let mut o = vec![rect((0, 0, w, h), Some(if chosen { mix(p[1], ac, 12) } else { p[1] }), Some(if chosen { ac } else { p[2] }), 12)];
    o.push(rect((10, (h - 26) / 2, 36, (h + 26) / 2), Some(col), None, 13));
    o.push(text((10, (h - 26) / 2, 36, (h + 26) / 2), &name[..1], 13, 700, ink(col), Align::Center));
    o.push(text((46, 0, w - 28, h), name, 14, 600, p[3], Align::Left));
    if chosen {
        o.push(Op::Poly { x: w - 24, y: h / 2 - 8, size: 16, pts: vec![(5, 12), (10, 17), (19, 7)], color: ac });
    }
    o
}
pub const AVATAR: [u32; 8] = [0xf26558, 0x6bd325, 0xd9a22a, 0x9a6e5e, 0xff8c2d, 0x3a1ee0, 0xf0763a, 0x4a9cf0];

#[cfg(test)]
mod tests {
    use super::*;

    /// The real theme tables, read from polished.rs so the dump uses the app's own colors.
    fn themes() -> [[u32; 5]; 7] {
        let src = include_str!("polished.rs");
        let body = src.split("pub const THEMES").nth(1).unwrap().split("];\npub const ACCENTS").next().unwrap();
        let mut out = [[0u32; 5]; 7];
        let mut n = 0;
        for line in body.lines().filter(|l| l.trim_start().starts_with("[0x")) {
            let hex: Vec<u32> = line.split("0x").skip(1).map(|t| u32::from_str_radix(&t[..6], 16).unwrap()).collect();
            out[n].copy_from_slice(&hex[..5]);
            n += 1;
        }
        assert_eq!(n, 6);
        out[6] = crate::custom_theme::palette(crate::custom_theme::DEFAULT);
        out
    }
    fn accents() -> [u32; 8] {
        let src = include_str!("polished.rs");
        let body = src.split("pub const ACCENTS: [u32; 8] = [").nth(1).unwrap().split("];").next().unwrap();
        let v: Vec<u32> = body.split("0x").skip(1).map(|t| u32::from_str_radix(&t[..6], 16).unwrap()).collect();
        let mut o = [0u32; 8];
        o.copy_from_slice(&v[..8]);
        o
    }
    fn view(step: usize, layout: usize, theme: usize, accent: usize, apps: &str) -> View {
        View { step, layout, theme, accent, themes: themes(), accents: accents(), mic: true, apps: apps.into(), pos: 0, preset: Some(1), name: "ZimClip".into(), icon: 0, has_upload: false, summary: vec!["Wide".into(), "Plum / Amber".into(), "System on / Mic on".into(), "discord".into(), "Alt+G".into(), "Balanced, 1280 wide, 30 fps".into(), "30 s, share 20 MB".into(), "ZimClip".into()] }
    }
    fn json(ops: &[Op], ox: i32, oy: i32) -> String {
        let mut s = Vec::new();
        for op in ops {
            s.push(match op {
                Op::Rect { r, fill, stroke, radius } => format!(
                    "{{\"k\":\"r\",\"r\":[{},{},{},{}],\"f\":{},\"s\":{},\"rad\":{}}}",
                    r.0 + ox, r.1 + oy, r.2 + ox, r.3 + oy,
                    fill.map_or("null".into(), |c| c.to_string()), stroke.map_or("null".into(), |c| c.to_string()), radius),
                Op::Text { r, s, px, weight, color, align } => format!(
                    "{{\"k\":\"t\",\"r\":[{},{},{},{}],\"s\":{:?},\"px\":{},\"w\":{},\"c\":{},\"a\":{}}}",
                    r.0 + ox, r.1 + oy, r.2 + ox, r.3 + oy, s, px, weight, color, *align as i32),
                Op::Poly { x, y, size, pts, color } => format!(
                    "{{\"k\":\"p\",\"x\":{},\"y\":{},\"z\":{},\"pts\":{},\"c\":{}}}",
                    x + ox, y + oy, size, format!("[{}]", pts.iter().map(|(a, b)| format!("[{a},{b}]")).collect::<Vec<_>>().join(",")), color),
            });
        }
        s.join(",")
    }

    #[test]
    fn dump_review_frames() {
        let Ok(dir) = std::env::var("ZIM_DUMP") else { return };
        let mut extra = view(6, 0, 0, 0, "");
        extra.pos = 0;
        let mut icons = view(7, 0, 0, 0, "");
        icons.pos = 1;
        icons.icon = 5;
        let mut presets = view(4, 0, 0, 0, "");
        presets.pos = 7;
        let mut summary = view(5, 4, 2, 3, "");
        summary.pos = 8;
        let frames: [(&str, View); 10] = [
            ("welcome", extra),
            ("name-icon", icons),
            ("hotkey-presets", presets),
            ("layout-balanced", view(0, 0, 0, 0, "")),
            ("layout-stacked-midnight", view(0, 3, 1, 0, "")),
            ("theme-charcoal", view(1, 0, 0, 0, "")),
            ("theme-midnight-sky", view(1, 0, 1, 1, "")),
            ("apps", view(3, 0, 0, 0, "discord, whatsapp.root, myapp")),
            ("layout-sidebar", view(0, 4, 0, 0, "")),
            ("summary", summary),
        ];
        for (name, v) in frames {
            let mut parts = vec![json(&base(&v), 0, 0)];
            if v.step == 3 {
                for i in 0..crate::welcome_logic::COMMON_APPS.len() {
                    let r = chip_rect(i);
                    let chosen = crate::welcome_logic::group_selected(&v.apps, crate::welcome_logic::COMMON_APPS[i].1);
                    parts.push(json(&chip(&v, i, chosen, r.2 - r.0, r.3 - r.1), r.0, r.1));
                }
            }
            std::fs::write(format!("{dir}/{name}.json"), format!("[{}]", parts.join(","))).unwrap();
        }
    }

    #[test]
    fn page_notes_fit_one_line_and_stay_above_the_hint() {
        for step in [2usize, 8] {
            let ops = base(&view(step, 0, 0, 0, ""));
            for o in &ops {
                if let Op::Text { r, s, px: 12, .. } = o {
                    if r.1 >= 300 && r.1 < 480 {
                        // Generous 6.6 px per character at 12 px.
                        assert!(s.chars().count() as i32 * 66 / 10 <= r.2 - r.0, "{s}");
                        assert!(r.3 <= 480, "{s}");
                    }
                }
            }
        }
    }

    #[test]
    fn accent_dots_hit_test() {
        for i in 0..8 {
            let (cx, cy) = accent_dot(i);
            assert_eq!(accent_at(cx, cy), Some(i as usize));
        }
        assert_eq!(accent_at(0, 0), None);
    }

    #[test]
    fn chips_fit_and_do_not_overlap() {
        for i in 0..8 {
            let a = chip_rect(i);
            assert!(a.2 <= 716 && a.3 < 372);
            for j in 0..i {
                let b = chip_rect(j);
                assert!(a.2 <= b.0 || b.2 <= a.0 || a.3 <= b.1 || b.3 <= a.1);
            }
        }
    }

    #[test]
    fn every_page_has_a_header_and_stays_inside_the_window() {
        for step in 0..6 {
            for op in base(&view(step, 0, 0, 0, "")) {
                if let Op::Rect { r, .. } | Op::Text { r, .. } = op {
                    assert!(r.0 >= 0 && r.2 <= 744 && r.3 <= 600, "step {step}: {r:?}");
                }
            }
        }
    }

    #[test]
    fn layout_page_is_roomy_and_inside_the_window() {
        let mut prev_right = -1;
        for i in 0..LAYOUT_COUNT {
            let (l, r) = crate::welcome_logic::card_span(i, 28, 688, LAYOUT_COUNT, LAYOUT_GAP);
            assert!(l > prev_right && r <= 716 && r - l >= 120);
            prev_right = r;
        }
        assert!(LAYOUT_H >= 160 && LAYOUT_TOP + LAYOUT_H + 12 <= 328);
        for op in base(&view(0, 4, 0, 0, "")) {
            if let Op::Rect { r, .. } | Op::Text { r, .. } = op {
                if r.1 >= 130 && r.1 < 500 {
                    assert!(r.2 <= 744 && r.3 <= 480, "{r:?}");
                }
            }
        }
    }

    #[test]
    fn summary_cards_fit_and_do_not_overlap() {
        for i in 0..8 {
            let a = summary_card(i);
            assert!(a.0 >= 28 && a.2 <= 716 && a.1 > 123 && a.3 <= 482, "{a:?}");
            for j in 0..i {
                let b = summary_card(j);
                assert!(a.2 <= b.0 || b.2 <= a.0 || a.3 <= b.1 || b.3 <= a.1);
            }
        }
        let ops = base(&view(5, 0, 0, 0, ""));
        let values = ops.iter().filter(|o| matches!(o, Op::Text { s, px: 16, .. } if !s.is_empty())).count();
        assert_eq!(values, 8);
    }

    #[test]
    fn native_fields_keep_a_gap_from_the_text_above_them() {
        // Hotkey page: two lines of help text, then the field, then the Capture size label.
        assert!(HOTKEY_FIELD.1 - HOTKEY_TEXT.3 >= 16);
        assert!(258 - HOTKEY_FIELD.3 >= 16);
        let first = preset_rect(0);
        assert!(first.1 - 284 >= 8 && first.3 <= 396);
        // Name page: label above the field, field full width, then the divider and Icon label.
        assert!(NAME_FIELD.1 - 152 >= 4 && NAME_FIELD.2 - NAME_FIELD.0 == 688);
        assert!(214 - NAME_FIELD.3 >= 12);
        assert!(icon_tile(0).1 >= 226 + 26);
        assert!(upload_rect().3 <= 482);
    }

    #[test]
    fn app_chips_are_taller_and_stay_above_the_text_box() {
        for i in 0..8 {
            let c = chip_rect(i);
            assert!(c.3 - c.1 >= 56 && c.3 <= 358);
        }
    }
}
