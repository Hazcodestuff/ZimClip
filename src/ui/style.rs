//! Shared native drawing parts for the staged redesign.
//! Additive, no capture, playback, persistence, timers or control IDs here.
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*};

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: u32,
    pub panel: u32,
    pub raise: u32,
    pub hover: u32,
    pub border: u32,
    pub line: u32,
    pub text: u32,
    pub muted: u32,
    pub dim: u32,
    pub accent: u32,
    pub ink: u32,
    pub amber: u32,
    pub rec: u32,
}
pub fn mix(a: u32, b: u32, percent: u32) -> u32 {
    let p = percent.min(100);
    let mut c = 0;
    for shift in [0, 8, 16] {
        c |= ((a >> shift & 255) * (100 - p) + (b >> shift & 255) * p) / 100 << shift;
    }
    c
}
fn luminance(c: u32) -> f64 {
    let channel = |x: u32| {
        let v = x as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c & 255) + 0.7152 * channel(c >> 8 & 255) + 0.0722 * channel(c >> 16 & 255)
}
pub fn contrast(a: u32, b: u32) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn readable(mut fg: u32, bg: u32, min: f64) -> u32 {
    let target = if luminance(bg) < 0.4 { 0xffffff } else { 0 };
    for _ in 0..32 {
        if contrast(fg, bg) >= min {
            break;
        }
        fg = mix(fg, target, 15);
    }
    fg
}
impl Theme {
    pub fn derive(bg: u32, panel: u32, border: u32, text: u32, muted: u32, accent: u32) -> Self {
        let raise = mix(panel, text, 6);
        let hover = mix(raise, accent, 12);
        Self {
            bg,
            panel,
            raise,
            hover,
            border,
            line: mix(panel, border, 65),
            text: readable(text, raise, 4.5),
            muted: readable(muted, raise, 4.5),
            dim: readable(mix(muted, panel, 18), raise, 4.5),
            accent,
            ink: if contrast(0x000000, accent) >= contrast(0xffffff, accent) {
                0
            } else {
                0xffffff
            },
            amber: readable(mix(accent, 0x60bbef, 75), panel, 4.5),
            rec: readable(mix(accent, 0x7070ee, 85), panel, 4.5),
        }
    }
}
#[derive(Clone, Copy, Default)]
pub struct State {
    pub hover: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
    pub selected: bool,
    pub error: bool,
    pub loading: bool,
}
#[derive(Clone, Copy)]
pub enum Button {
    Primary,
    Default,
    Ghost,
    Record,
    Icon,
    Save,
    Danger,
}

// Objects are process-lifetime cached. Mutexes guard only map access, never UI calls.
// Selection/restoration is the caller's drawing scope; cached objects are never deleted.
fn brushes() -> &'static Mutex<BTreeMap<u32, usize>> {
    static C: OnceLock<Mutex<BTreeMap<u32, usize>>> = OnceLock::new();
    C.get_or_init(Default::default)
}
fn pens() -> &'static Mutex<BTreeMap<(u32, i32), usize>> {
    static C: OnceLock<Mutex<BTreeMap<(u32, i32), usize>>> = OnceLock::new();
    C.get_or_init(Default::default)
}
fn fonts() -> &'static Mutex<BTreeMap<(String, i32, i32), usize>> {
    static C: OnceLock<Mutex<BTreeMap<(String, i32, i32), usize>>> = OnceLock::new();
    C.get_or_init(Default::default)
}
pub unsafe fn brush(color: u32) -> HBRUSH {
    if let Some(v) = brushes().lock().unwrap().get(&color).copied() {
        return v as _;
    }
    let value = CreateSolidBrush(color) as usize;
    let mut cache = brushes().lock().unwrap();
    if let Some(old) = cache.get(&color) {
        DeleteObject(value as _);
        return *old as _;
    }
    cache.insert(color, value);
    value as _
}
pub unsafe fn pen(color: u32, width: i32) -> HPEN {
    let key = (color, width);
    if let Some(v) = pens().lock().unwrap().get(&key).copied() {
        return v as _;
    }
    let value = CreatePen(PS_SOLID, width, color) as usize;
    let mut cache = pens().lock().unwrap();
    if let Some(old) = cache.get(&key) {
        DeleteObject(value as _);
        return *old as _;
    }
    cache.insert(key, value);
    value as _
}
pub unsafe fn font(face: &str, px: i32, weight: i32) -> HFONT {
    let key = (face.to_string(), px.max(12), weight);
    if let Some(v) = fonts().lock().unwrap().get(&key).copied() {
        return v as _;
    }
    let name: Vec<u16> = face.encode_utf16().chain(Some(0)).collect();
    let value = CreateFontW(
        -key.1,
        0,
        0,
        0,
        weight as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as _,
        0,
        0,
        CLEARTYPE_QUALITY as _,
        0,
        name.as_ptr(),
    ) as usize;
    let mut cache = fonts().lock().unwrap();
    if let Some(old) = cache.get(&key) {
        DeleteObject(value as _);
        return *old as _;
    }
    cache.insert(key, value);
    value as _
}
pub unsafe fn warm(t: Theme, face: &str, scale: f32) {
    for c in [
        t.bg,
        t.panel,
        t.raise,
        t.hover,
        t.border,
        t.line,
        t.text,
        t.muted,
        t.dim,
        t.accent,
        t.ink,
        t.amber,
        t.rec,
        mix(t.panel, t.amber, 10),
        mix(t.panel, t.rec, 10),
    ] {
        brush(c);
        pen(c, 1);
        pen(c, 2);
    }
    // Every state-derived colour is prepared before a screen becomes visible.
    for c in [
        mix(t.accent, t.ink, 14),
        mix(t.panel, t.accent, 12),
        mix(t.panel, t.amber, 12),
        mix(t.panel, t.amber, 35),
        mix(t.panel, t.rec, 35),
        readable(t.amber, mix(t.panel, t.amber, 10), 4.5),
        readable(t.rec, mix(t.panel, t.rec, 10), 4.5),
    ] {
        brush(c);
        pen(c, 1);
        pen(c, 2);
    }
    for (px, w) in [
        (12, 400),
        (13, 400),
        (14, 400),
        (14, 600),
        (18, 600),
        (24, 600),
        (26, 600),
    ] {
        font(face, ((px as f32) * scale).round() as i32, w);
    }
}
/// Cached BGRA bitmap holding an anti-aliased 2r x 2r rounded square, used for the four
/// corners of `rect`. Keyed by radius and colours; bounded so hover colour fades cannot
/// grow it without limit.
unsafe fn corner_bitmap(rad: i32, fill: u32, stroke: u32) -> Option<HBITMAP> {
    static CACHE: Mutex<Option<std::collections::HashMap<(i32, u32, u32), isize>>> = Mutex::new(None);
    let mut guard = CACHE.lock().ok()?;
    let map = guard.get_or_insert_with(Default::default);
    if let Some(h) = map.get(&(rad, fill, stroke)) {
        return Some(*h as HBITMAP);
    }
    if map.len() >= 400 {
        for (_, h) in map.drain() {
            DeleteObject(h as _);
        }
    }
    let side = (rad * 2) as usize;
    let px = crate::aa::rrect(side, side, rad, Some(fill), Some(stroke));
    let bmp = dib(&px)?;
    map.insert((rad, fill, stroke), bmp as isize);
    Some(bmp)
}
/// Top-down 32-bit DIB holding premultiplied BGRA pixels.
pub unsafe fn dib(p: &crate::aa::Px) -> Option<HBITMAP> {
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = p.w as i32;
    bi.bmiHeader.biHeight = -(p.h as i32);
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(std::ptr::null_mut(), &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if bmp.is_null() || bits.is_null() {
        return None;
    }
    std::ptr::copy_nonoverlapping(p.data.as_ptr(), bits as *mut u8, p.data.len());
    Some(bmp)
}
/// Alpha-blend a pixel buffer onto `dc` at (x, y).
pub unsafe fn blit_px(dc: HDC, x: i32, y: i32, p: &crate::aa::Px) {
    let Some(bmp) = dib(p) else { return };
    let mem = CreateCompatibleDC(dc);
    let old = SelectObject(mem, bmp as _);
    let f = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
    AlphaBlend(dc, x, y, p.w as i32, p.h as i32, mem, 0, 0, p.w as i32, p.h as i32, f);
    SelectObject(mem, old);
    DeleteDC(mem);
    DeleteObject(bmp as _);
}
/// Rounded rectangle with anti-aliased corners. Straight edges are plain fills (they have
/// nothing to smooth); the four corners are alpha-blended from a cached coverage bitmap, so
/// the cost does not grow with the size of the rectangle. Same inside-edge 1px outline as
/// the GDI version it replaces.
pub unsafe fn rect(dc: HDC, r: RECT, fill: u32, stroke: u32, radius: i32) {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let rad = radius.min(w / 2).min(h / 2);
    let bmp = if rad >= 3 { corner_bitmap(rad, fill, stroke) } else { None };
    let Some(bmp) = bmp else {
        let b = SelectObject(dc, brush(fill) as _);
        let p = SelectObject(dc, pen(stroke, 1) as _);
        RoundRect(dc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
        SelectObject(dc, b);
        SelectObject(dc, p);
        return;
    };
    let (l, t, rt, b) = (r.left, r.top, r.right, r.bottom);
    let fb = brush(fill);
    let sb = brush(stroke);
    let fr = |a: i32, c: i32, d: i32, e: i32, br| {
        if d > a && e > c {
            FillRect(dc, &RECT { left: a, top: c, right: d, bottom: e }, br);
        }
    };
    fr(l + rad, t, rt - rad, b, sb);
    fr(l + rad, t + 1, rt - rad, b - 1, fb);
    fr(l, t + rad, l + rad, b - rad, sb);
    fr(l + 1, t + rad, l + rad, b - rad, fb);
    fr(rt - rad, t + rad, rt, b - rad, sb);
    fr(rt - rad, t + rad, rt - 1, b - rad, fb);
    let mem = CreateCompatibleDC(dc);
    let old = SelectObject(mem, bmp as _);
    let f = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
    for (dx, dy, sx, sy) in [(l, t, 0, 0), (rt - rad, t, rad, 0), (l, b - rad, 0, rad), (rt - rad, b - rad, rad, rad)] {
        AlphaBlend(dc, dx, dy, rad, rad, mem, sx, sy, rad, rad, f);
    }
    SelectObject(mem, old);
    DeleteDC(mem);
}
/// Anti-aliased filled circle (or ellipse) in `r`, with an optional 1px outline colour.
pub unsafe fn disc(dc: HDC, r: RECT, fill: u32, stroke: Option<u32>) {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w < 2 || h < 2 {
        return;
    }
    let px = crate::aa::rrect(w as usize, h as usize, w.min(h) / 2, Some(fill), Some(stroke.unwrap_or(fill)));
    blit_px(dc, r.left, r.top, &px);
}
/// Anti-aliased open polyline, `width` px wide with round caps, in client coordinates.
pub unsafe fn lines(dc: HDC, pts: &[(i32, i32)], width: f32, color: u32) {
    if let Some((px, ox, oy)) = crate::aa::polyline(pts, width, color) {
        blit_px(dc, ox, oy, &px);
    }
}
pub unsafe fn label(dc: HDC, s: &str, mut r: RECT, font: HFONT, color: u32, flags: u32) {
    if s.is_empty() {
        return;
    }
    let f = SelectObject(dc, font as _);
    SetBkMode(dc, TRANSPARENT as _);
    SetTextColor(dc, color);
    let text: Vec<u16> = s.encode_utf16().collect();
    DrawTextW(
        dc,
        text.as_ptr(),
        text.len() as _,
        &mut r,
        flags | DT_NOPREFIX,
    );
    SelectObject(dc, f);
}
pub unsafe fn focus(dc: HDC, r: RECT, t: Theme) {
    let p = SelectObject(dc, pen(t.accent, 2) as _);
    let b = SelectObject(dc, GetStockObject(NULL_BRUSH));
    RoundRect(dc, r.left + 2, r.top + 2, r.right - 2, r.bottom - 2, 20, 20);
    SelectObject(dc, p);
    SelectObject(dc, b);
}
pub unsafe fn button(dc: HDC, r: RECT, title: &str, kind: Button, s: State, t: Theme, f: HFONT) {
    let primary = matches!(kind, Button::Primary | Button::Save);
    let ghost = matches!(kind, Button::Ghost);
    let error = s.error || matches!(kind, Button::Danger);
    let fill = if s.disabled {
        t.raise
    } else if primary {
        if s.pressed {
            mix(t.accent, t.ink, 14)
        } else {
            t.accent
        }
    } else if s.pressed || s.hover {
        t.hover
    } else if ghost {
        t.panel
    } else if matches!(kind, Button::Record) && s.selected {
        mix(t.panel, t.rec, 10)
    } else {
        t.raise
    };
    let edge = if error {
        t.rec
    } else if primary || ghost {
        fill
    } else {
        t.border
    };
    if matches!(kind, Button::Save) {
        smooth_save(dc, r, fill, edge, t.panel);
    } else {
        rect(dc, r, fill, edge, 10);
    }

    let color = if s.disabled {
        t.dim
    } else if primary {
        t.ink
    } else if error {
        t.rec
    } else if ghost {
        t.muted
    } else {
        t.text
    };
    let title = if s.loading { "Working..." } else { title };
    let mut title_rect = r;
    if matches!(kind, Button::Record) {
        title_rect.left += 20;
    }
    label(
        dc,
        title,
        title_rect,
        f,
        color,
        DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_END_ELLIPSIS,
    );
    if matches!(kind, Button::Record) {
        let x = r.left + 14;
        let b = SelectObject(dc, brush(if s.disabled { t.dim } else { t.rec }) as _);
        let p = SelectObject(dc, pen(fill, 1) as _);
        Ellipse(dc, x, r.top + 16, x + 8, r.top + 24);
        SelectObject(dc, b);
        SelectObject(dc, p);
    }
    if s.focused {
        focus(dc, r, t)
    }
}
pub unsafe fn switch(dc: HDC, r: RECT, on: bool, s: State, t: Theme) {
    switch_at(dc, r, if on { 1. } else { 0. }, s, t)
}
pub unsafe fn switch_at(dc: HDC, r: RECT, progress: f32, s: State, t: Theme) {
    let progress = progress.clamp(0., 1.);
    let on = progress >= 0.5;
    let x = r.right - 48;
    let y = r.top + (r.bottom - r.top - 24) / 2;
    let color = if s.disabled {
        t.border
    } else if progress > 0. {
        mix(
            t.border,
            t.accent,
            ((progress * 12.).round() as u32) * 100 / 12,
        )
    } else if s.hover {
        t.hover
    } else {
        t.border
    };
    rect(
        dc,
        RECT {
            left: x,
            top: y,
            right: x + 44,
            bottom: y + 24,
        },
        color,
        color,
        12,
    );
    let x = x + 3 + (20. * progress).round() as i32;
    // Light knob with a thin dark rim, drawn through the shared anti-aliased disc.
    let knob = if s.disabled { t.dim } else if on { 0xffffff } else { t.muted };
    disc(dc, RECT { left: x, top: y + 3, right: x + 18, bottom: y + 21 }, knob, Some(if on { t.bg } else { color }));
    if s.focused {
        focus(dc, r, t)
    }
}
pub unsafe fn keycap(dc: HDC, r: RECT, value: &str, t: Theme, f: HFONT) {
    rect(dc, r, t.raise, t.border, 7);
    label(
        dc,
        value,
        r,
        f,
        t.text,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}
pub unsafe fn pill(dc: HDC, r: RECT, value: &str, warning: bool, t: Theme, f: HFONT) {
    let color = if warning { t.amber } else { t.accent };
    rect(dc, r, mix(t.panel, color, 12), mix(t.panel, color, 12), 12);
    label(
        dc,
        value,
        r,
        f,
        color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}
pub unsafe fn note(dc: HDC, r: RECT, value: &str, error: bool, t: Theme, f: HFONT) {
    let color = if error { t.rec } else { t.amber };
    rect(dc, r, mix(t.panel, color, 10), mix(t.panel, color, 35), 10);
    let mut text = r;
    text.left += 12;
    text.right -= 12;
    text.top += 8;
    text.bottom -= 8;
    label(
        dc,
        value,
        text,
        f,
        readable(color, mix(t.panel, color, 10), 4.5),
        DT_WORDBREAK,
    );
}
pub unsafe fn chip(dc: HDC, r: RECT, value: &str, selected: bool, s: State, t: Theme, f: HFONT) {
    rect(
        dc,
        r,
        if s.hover && !s.disabled {
            t.hover
        } else {
            t.raise
        },
        if selected { t.accent } else { t.border },
        14,
    );
    label(
        dc,
        value,
        r,
        f,
        if s.disabled {
            t.dim
        } else if selected {
            t.accent
        } else {
            t.text
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    if s.focused {
        focus(dc, r, t)
    }
}
pub unsafe fn segments(
    dc: HDC,
    r: RECT,
    options: &[&str],
    selected: usize,
    s: State,
    t: Theme,
    f: HFONT,
) {
    rect(dc, r, t.panel, t.border, 10);
    if options.is_empty() {
        return;
    }
    let w = (r.right - r.left - 4) / options.len() as i32;
    for (i, title) in options.iter().enumerate() {
        let rr = RECT {
            left: r.left + 2 + i as i32 * w,
            top: r.top + 2,
            right: r.left + 2 + (i as i32 + 1) * w,
            bottom: r.bottom - 2,
        };
        if i == selected {
            rect(dc, rr, t.raise, t.raise, 8)
        }
        label(
            dc,
            title,
            rr,
            f,
            if s.disabled {
                t.dim
            } else if i == selected {
                t.accent
            } else {
                t.muted
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
    if s.focused {
        focus(dc, r, t)
    }
}
/// Passive renderer. Caller supplies measured levels and controls invalidation cadence.
/// None means unavailable, so no invented activity is drawn.
pub unsafe fn meter(dc: HDC, r: RECT, level: Option<f32>, on: bool, t: Theme) {
    let level = level.unwrap_or(0.).clamp(0., 1.);
    let w = ((r.right - r.left) / 8).max(2);
    for i in 0..8 {
        let h = ((r.bottom - r.top) * (i + 2) / 9).max(2);
        let fill = if on && (i as f32) < level * 8. {
            t.accent
        } else {
            t.border
        };
        rect(
            dc,
            RECT {
                left: r.left + i * w,
                top: r.bottom - h,
                right: r.left + i * w + (w - 2).max(1),
                bottom: r.bottom,
            },
            fill,
            fill,
            1,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contrast_and_mix() {
        assert_eq!(mix(0, 0xffffff, 100), 0xffffff);
        assert_eq!(mix(0, 0xffffff, 0), 0);
        assert!((contrast(0, 0xffffff) - 21.).abs() < 0.01);
    }
    #[test]
    fn derived_ink_is_readable() {
        for accent in [
            0xc6e89b, 0xf4c38d, 0xe8abcf, 0x84c6ed, 0xdbc06b, 0xa7d5ff, 0xe1c2ed, 0xf3f3f3,
        ] {
            let t = Theme::derive(0x171310, 0x211d19, 0x39332b, 0xf4f0e8, 0xaba396, accent);
            assert!(contrast(t.ink, t.accent) >= 4.5);
            assert!(contrast(t.text, t.raise) >= 4.5);
            assert!(contrast(t.muted, t.raise) >= 4.5);
        }
    }
}

// Small supersampled shape only. Text remains native at final resolution.
unsafe fn smooth_save(dc: HDC, r: RECT, fill: u32, edge: u32, bg: u32) {
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    if w <= 0 || h <= 0 {
        return;
    }
    let mem = CreateCompatibleDC(dc);
    let bmp = CreateCompatibleBitmap(dc, w * 3, h * 3);
    if mem.is_null() || bmp.is_null() {
        if !mem.is_null() {
            DeleteDC(mem);
        }
        if !bmp.is_null() {
            DeleteObject(bmp);
        }
        rect(dc, r, fill, edge, 14);
        return;
    }
    let old = SelectObject(mem, bmp);
    FillRect(
        mem,
        &RECT {
            left: 0,
            top: 0,
            right: w * 3,
            bottom: h * 3,
        },
        brush(bg),
    );
    rect(
        mem,
        RECT {
            left: 0,
            top: 0,
            right: w * 3,
            bottom: h * 3,
        },
        fill,
        edge,
        42,
    );
    let mode = SetStretchBltMode(dc, HALFTONE as _);
    let mut origin = POINT::default();
    SetBrushOrgEx(dc, 0, 0, &mut origin);
    StretchBlt(dc, r.left, r.top, w, h, mem, 0, 0, w * 3, h * 3, SRCCOPY);
    SetBrushOrgEx(dc, origin.x, origin.y, std::ptr::null_mut());
    SetStretchBltMode(dc, mode);
    SelectObject(mem, old);
    DeleteObject(bmp);
    DeleteDC(mem);
}
