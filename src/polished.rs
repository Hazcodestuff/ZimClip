//! Lightweight "polished" interface layer.
//!
//! This module only paints. It adds no windows, threads, timers, capture work or
//! persistent state beyond the single `ui=` preference line. Every shape uses the
//! process-lifetime GDI cache in `ui::style`, so switching screens never allocates
//! drawing resources. When the user selects Legacy mode every hook here returns
//! immediately and the original 0.22.5 rendering path is used unchanged.
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Controls::*, Input::KeyboardAndMouse::GetFocus, WindowsAndMessaging::*},
};

use crate::ui::style;

static ON: AtomicBool = AtomicBool::new(true);

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}
pub fn set(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// Read the interface preference. Missing or unknown values mean Polished.
pub fn load(root: &Path) {
    let text = std::fs::read_to_string(root.join("personalization.ini")).unwrap_or_default();
    let legacy = text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "ui").then(|| {
            let v = value.trim();
            v == "legacy" || v == "0"
        })
    });
    set(!legacy.unwrap_or(false));
    let reduce = text.lines().any(|l| l.split_once('=').is_some_and(|(k, v)| k.trim() == "motion" && v.trim() == "reduce"));
    crate::ui::motion::set_reduced(reduce);
}

pub fn motion_value() -> &'static str {
    if crate::ui::motion::reduced() { "reduce" } else { "full" }
}

/// All appearance toggles share one writer so the extra `ui` line survives every save.
pub fn save_prefs(root: &Path, theme: usize, hotkey: u32, sound: bool, toast: bool) -> Result<(), String> {
    let mode = if enabled() { "polished" } else { "legacy" };
    let body = format!(
        "theme={theme}\nhotkey={hotkey}\nsound={}\ntoast={}\nui={mode}\nmotion={}\n",
        sound as u8, toast as u8, motion_value()
    );
    let path = root.join("personalization.ini");
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub unsafe fn prop(h: HWND, key: &str) -> usize {
    GetPropW(h, wide(key).as_ptr()) as usize
}
unsafe fn text(h: HWND) -> String {
    let mut buf = vec![0u16; (GetWindowTextLengthW(h) + 1).max(1) as usize];
    let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as _);
    String::from_utf16_lossy(&buf[..n as usize])
}

/// Exact surface tokens from the reference sheet (BGR u32).
// GDI colours are 0x00BBGGRR; the reference sheet uses CSS 0xRRGGBB.
pub const THEMES: [[u32; 5]; 6] = [
    [0x120f0d, 0x1b1714, 0x38312a, 0xf4f1ee, 0xb3a9a0], // charcoal
    [0x170e0a, 0x241610, 0x4a3023, 0xfaf2ee, 0xc2ab9f], // midnight
    [0x160d12, 0x201219, 0x492a38, 0xf7eef3, 0xb89faa], // plum
    [0x0c0e13, 0x12141b, 0x22283a, 0xeceff6, 0x9ba2b3], // ember
    [0x0e110b, 0x151a11, 0x2e3a26, 0xf0f5ed, 0xa6b39d], // forest
    [0x000000, 0x0b0b0b, 0x666666, 0xffffff, 0xe6e6e6], // contrast
];
pub const ACCENTS: [u32; 8] = [
    0xc4e89b, 0xffc47c, 0xffa8c4, 0x6ac7f5, 0xf5e9bf, 0x8fb0f6, 0xbba0f4, 0xd9d4d0,
];
pub const OK: u32 = 0x8ae07b;
pub const BLUE: u32 = 0xffc47c;
pub const VIOLET: u32 = 0xffa8c4;
pub const WARN: u32 = 0x6bc2f2;
pub const REC: u32 = 0x7070ff;

pub fn palette(index: usize) -> [u32; 5] {
    if index % 7 == crate::custom_theme::THEME_INDEX {
        return crate::custom_theme::palette(crate::custom_theme::current());
    }
    THEMES[index % 6]
}
pub fn accent(index: usize) -> u32 {
    ACCENTS[index % 8]
}

/// Reference tokens mapped onto the shared Theme used by every redesigned screen.
pub fn theme(theme_index: usize, accent_index: usize) -> style::Theme {
    let p = palette(theme_index);
    let ac = accent(accent_index);
    let ink = if style::contrast(0, ac) >= style::contrast(0xffffff, ac) {
        0
    } else {
        0xffffff
    };
    style::Theme {
        bg: p[0],
        panel: p[1],
        raise: style::mix(p[1], p[3], 7),
        hover: style::mix(p[1], p[3], 13),
        border: p[2],
        line: style::mix(p[1], p[2], 55),
        text: p[3],
        muted: p[4],
        dim: style::mix(p[4], p[1], 26),
        accent: ac,
        ink,
        amber: WARN,
        rec: REC,
    }
}

unsafe fn font(px: i32, weight: i32) -> HFONT {
    style::font(&crate::display::font_name(), px, weight)
}

#[derive(Clone, Copy)]
struct State {
    hover: bool,
    pressed: bool,
    focused: bool,
    disabled: bool,
}
unsafe fn control_state(d: &DRAWITEMSTRUCT) -> State {
    let mut cursor = POINT::default();
    GetCursorPos(&mut cursor);
    let mut bounds = RECT::default();
    GetWindowRect(d.hwndItem, &mut bounds);
    State {
        hover: cursor.x >= bounds.left
            && cursor.x < bounds.right
            && cursor.y >= bounds.top
            && cursor.y < bounds.bottom,
        pressed: d.itemState & ODS_SELECTED != 0,
        focused: GetFocus() == d.hwndItem,
        disabled: d.itemState & ODS_DISABLED != 0,
    }
}

// ── Icons: 24x24 stroke glyphs, drawn with cached pens. No bitmaps, no GDI+. ──
unsafe fn stroke(dc: HDC, pts: &[(i32, i32)], x: i32, y: i32, s: i32, color: u32, close: bool) {
    let old = SelectObject(dc, style::pen(color, 2 * sc()) as _);
    let px = |v: i32| x + v * s / 24;
    let py = |v: i32| y + v * s / 24;
    MoveToEx(dc, px(pts[0].0), py(pts[0].1), std::ptr::null_mut());
    for (a, b) in &pts[1..] {
        LineTo(dc, px(*a), py(*b));
    }
    if close {
        LineTo(dc, px(pts[0].0), py(pts[0].1));
    }
    SelectObject(dc, old);
}
unsafe fn circle(dc: HDC, x: i32, y: i32, s: i32, color: u32) {
    let old = SelectObject(dc, style::brush(color) as _);
    let p = SelectObject(dc, style::pen(color, sc()) as _);
    Ellipse(dc, x + s * 8 / 24, y + s * 8 / 24, x + s * 16 / 24, y + s * 16 / 24);
    SelectObject(dc, old);
    SelectObject(dc, p);
}
/// Line-width multiplier while an icon is drawn at supersampled size.
static SCALE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);
fn sc() -> i32 {
    SCALE.load(std::sync::atomic::Ordering::Relaxed)
}
const SUPER: i32 = 4;
/// App identity in the header tile: default bolt, a chosen built-in glyph, or the
/// uploaded image drawn over the tile. Same choice the title bar and tray use.
unsafe fn brand_glyph(dc: HDC, tile: RECT, ink: u32) {
    let choice = crate::identity::icon_choice();
    if choice == crate::identity::custom_choice() {
        if let Some(h) = crate::ui::identity_icon() {
            windows_sys::Win32::UI::WindowsAndMessaging::DrawIconEx(
                dc,
                tile.left,
                tile.top,
                h as _,
                tile.right - tile.left,
                tile.bottom - tile.top,
                0,
                std::ptr::null_mut(),
                windows_sys::Win32::UI::WindowsAndMessaging::DI_NORMAL,
            );
            return;
        }
    }
    let name = if choice == 0 || choice > crate::welcome_draw::GLYPH_ICONS { "bolt".to_string() } else { format!("brand{choice}") };
    icon(dc, &name, tile.left + 10, tile.top + 10, 22, ink);
}

/// Icon drawn anti-aliased: the glyph is drawn 4x larger with GDI, averaged down to coverage,
/// and alpha-blended in `color`. Cached per (name, size, color).
pub unsafe fn icon(dc: HDC, name: &str, x: i32, y: i32, s: i32, color: u32) {
    use std::{collections::HashMap, sync::Mutex};
    // Coverage per (name, size) is cached; the tint is applied per draw, so colors that
    // change during a hover transition do not grow the cache.
    static CACHE: Mutex<Option<HashMap<(String, i32), Vec<u8>>>> = Mutex::new(None);
    if s <= 0 {
        return;
    }
    let key = (name.to_string(), s);
    let cached = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(&key).cloned();
    let cover = match cached {
        Some(c) => c,
        None => {
            let Some(c) = coverage(name, s) else {
                icon_raw(dc, name, x, y, s, color);
                return;
            };
            CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, c.clone());
            c
        }
    };
    let Some((bmp, bits)) = dib(s) else { return };
    let dst = std::slice::from_raw_parts_mut(bits, (s * s * 4) as usize);
    let (r, g, b) = ((color & 255) as u32, (color >> 8 & 255) as u32, (color >> 16 & 255) as u32);
    for (i, a) in cover.iter().enumerate() {
        let a = *a as u32;
        dst[i * 4] = (b * a / 255) as u8;
        dst[i * 4 + 1] = (g * a / 255) as u8;
        dst[i * 4 + 2] = (r * a / 255) as u8;
        dst[i * 4 + 3] = a as u8;
    }
    let mem = CreateCompatibleDC(dc);
    let old = SelectObject(mem, bmp as _);
    let f = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
    AlphaBlend(dc, x, y, s, s, mem, 0, 0, s, s, f);
    SelectObject(mem, old);
    DeleteDC(mem);
    DeleteObject(bmp as _);
}
unsafe fn dib(side: i32) -> Option<(HBITMAP, *mut u8)> {
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = side;
    bi.bmiHeader.biHeight = -side;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let b = CreateDIBSection(std::ptr::null_mut(), &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    (!b.is_null() && !bits.is_null()).then_some((b, bits as *mut u8))
}
/// Glyph coverage (0..255 per pixel, s x s): drawn SUPER times larger, then averaged down.
unsafe fn coverage(name: &str, s: i32) -> Option<Vec<u8>> {
    let big = s * SUPER;
    let (bb, bbits) = dib(big)?;
    let dc = CreateCompatibleDC(std::ptr::null_mut());
    let old = SelectObject(dc, bb as _);
    SCALE.store(SUPER, std::sync::atomic::Ordering::Relaxed);
    icon_raw(dc, name, 0, 0, big, 0xffffff);
    SCALE.store(1, std::sync::atomic::Ordering::Relaxed);
    GdiFlush();
    let src = std::slice::from_raw_parts(bbits, (big * big * 4) as usize);
    let mut out = vec![0u8; (s * s) as usize];
    for y in 0..s {
        for x in 0..s {
            let mut sum = 0u32;
            for j in 0..SUPER {
                for i in 0..SUPER {
                    sum += src[(((y * SUPER + j) * big + x * SUPER + i) * 4 + 1) as usize] as u32;
                }
            }
            out[(y * s + x) as usize] = (sum / (SUPER * SUPER) as u32) as u8;
        }
    }
    SelectObject(dc, old);
    DeleteDC(dc);
    DeleteObject(bb as _);
    Some(out)
}
unsafe fn icon_raw(dc: HDC, name: &str, x: i32, y: i32, s: i32, color: u32) {
    // "brandN": the user's chosen built-in app icon N (same outlines as setup).
    if let Some(n) = name.strip_prefix("brand").and_then(|n| n.parse::<usize>().ok()) {
        for pts in crate::welcome_draw::icon_strokes(n) {
            if pts.len() > 1 {
                stroke(dc, &pts, x, y, s, color, false);
            }
        }
        return;
    }
    match name {
        "bolt" => {
            stroke(
                dc,
                &[(13, 2), (4, 14), (11, 14), (10, 22), (20, 9), (13, 9)],
                x,
                y,
                s,
                color,
                true,
            );
        }
        "download" => {
            stroke(dc, &[(12, 3), (12, 15), (7, 10), (12, 15), (17, 10)], x, y, s, color, false);
            stroke(dc, &[(4, 19), (20, 19)], x, y, s, color, false);
        }
        "folder" => stroke(
            dc,
            &[(3, 6), (10, 6), (13, 9), (21, 9), (21, 19), (3, 19), (3, 6)],
            x,
            y,
            s,
            color,
            false,
        ),
        "video" => {
            stroke(dc, &[(3, 7), (15, 7), (15, 17), (3, 17), (3, 7)], x, y, s, color, false);
            stroke(dc, &[(15, 11), (21, 7), (21, 17), (15, 13)], x, y, s, color, false);
        }
        "play" => stroke(dc, &[(8, 5), (19, 12), (8, 19), (8, 5)], x, y, s, color, true),
        "pause" => {
            stroke(dc, &[(9, 5), (9, 19)], x, y, s, color, false);
            stroke(dc, &[(15, 5), (15, 19)], x, y, s, color, false);
        }
        "speaker" => stroke(
            dc,
            &[(4, 9), (8, 9), (13, 5), (13, 19), (8, 15), (4, 15), (4, 9)],
            x,
            y,
            s,
            color,
            true,
        ),
        "mic" => {
            let old = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = SelectObject(dc, style::pen(color, 2 * sc()) as _);
            RoundRect(dc, x + s * 9 / 24, y + s * 3 / 24, x + s * 15 / 24, y + s * 14 / 24, s / 5, s / 5);
            SelectObject(dc, old);
            SelectObject(dc, p);
            stroke(
                dc,
                &[(5, 11), (6, 15), (9, 17), (12, 18), (15, 17), (18, 15), (19, 11)],
                x,
                y,
                s,
                color,
                false,
            );
            stroke(dc, &[(12, 18), (12, 22)], x, y, s, color, false);
        }
        "headset" => {
            stroke(dc, &[(4, 15), (4, 12), (7, 6), (12, 4), (17, 6), (20, 12), (20, 15)], x, y, s, color, false);
            let old = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = SelectObject(dc, style::pen(color, 2 * sc()) as _);
            RoundRect(dc, x + s * 2 / 24, y + s * 14 / 24, x + s * 7 / 24, y + s * 21 / 24, s / 8, s / 8);
            RoundRect(dc, x + s * 17 / 24, y + s * 14 / 24, x + s * 22 / 24, y + s * 21 / 24, s / 8, s / 8);
            SelectObject(dc, old);
            SelectObject(dc, p);
        }
        "search" => {
            let old = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = SelectObject(dc, style::pen(color, 2 * sc()) as _);
            Ellipse(dc, x + s * 4 / 24, y + s * 4 / 24, x + s * 17 / 24, y + s * 17 / 24);
            SelectObject(dc, old);
            SelectObject(dc, p);
            stroke(dc, &[(17, 17), (21, 21)], x, y, s, color, false);
        }
        "refresh" => {
            stroke(dc, &[(20, 11), (18, 6), (14, 3), (8, 4), (4, 9), (4, 14), (7, 19), (13, 21), (18, 19)], x, y, s, color, false);
            stroke(dc, &[(20, 5), (20, 11), (14, 11)], x, y, s, color, false);
        }
        "tag" => stroke(dc, &[(3, 12), (3, 4), (11, 4), (21, 13), (12, 21), (3, 12)], x, y, s, color, true),
        "check" => stroke(dc, &[(5, 12), (10, 17), (19, 7)], x, y, s, color, false),
        "restart" => {
            stroke(dc, &[(19, 13), (18, 18), (14, 21), (8, 20), (5, 15), (6, 9), (10, 5), (16, 5)], x, y, s, color, false);
            stroke(dc, &[(20, 4), (20, 11), (13, 11)], x, y, s, color, false);
            stroke(dc, &[(12, 10), (16, 13), (12, 16)], x, y, s, color, false);
        }
        "pulse" => stroke(dc, &[(3, 12), (7, 12), (10, 6), (14, 18), (17, 12), (21, 12)], x, y, s, color, false),
        "gear" => {
            let old = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = SelectObject(dc, style::pen(color, 2 * sc()) as _);
            Ellipse(dc, x + s * 9 / 24, y + s * 9 / 24, x + s * 15 / 24, y + s * 15 / 24);
            SelectObject(dc, old);
            SelectObject(dc, p);
            for (ax, ay, bx, by) in [(12, 2, 12, 6), (12, 18, 12, 22), (2, 12, 6, 12), (18, 12, 22, 12), (5, 5, 8, 8), (16, 16, 19, 19), (5, 19, 8, 16), (19, 5, 16, 8)] {
                stroke(dc, &[(ax, ay), (bx, by)], x, y, s, color, false);
            }
        }
        "info" => {
            let old = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = SelectObject(dc, style::pen(color, 2 * sc()) as _);
            Ellipse(dc, x + s * 3 / 24, y + s * 3 / 24, x + s * 21 / 24, y + s * 21 / 24);
            SelectObject(dc, old);
            SelectObject(dc, p);
            stroke(dc, &[(12, 11), (12, 17)], x, y, s, color, false);
            circle(dc, x, y + s / 6, s, color);
        }
        "trash" => {
            stroke(dc, &[(4, 7), (20, 7), (17, 21), (7, 21), (4, 7)], x, y, s, color, false);
            stroke(dc, &[(9, 7), (9, 4), (15, 4), (15, 7)], x, y, s, color, false);
        }
        "back" => stroke(dc, &[(15, 5), (8, 12), (15, 19)], x, y, s, color, false),
        "edit" => {
            stroke(dc, &[(4, 20), (5, 15), (17, 4), (20, 7), (8, 19), (4, 20)], x, y, s, color, true);
        }
        "scissors" => {
            let old = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = SelectObject(dc, style::pen(color, 2 * sc()) as _);
            Ellipse(dc, x, y, x + s * 5 / 24, y + s * 5 / 24);
            Ellipse(dc, x, y + s * 15 / 24, x + s * 5 / 24, y + s * 20 / 24);
            SelectObject(dc, old);
            SelectObject(dc, p);
            stroke(dc, &[(6, 7), (20, 19), (6, 17), (20, 5)], x, y, s, color, false);
        }
        "select" => {
            let old = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = SelectObject(dc, style::pen(color, 2 * sc()) as _);
            RoundRect(dc, x + s * 4 / 24, y + s * 4 / 24, x + s * 20 / 24, y + s * 20 / 24, s / 8, s / 8);
            SelectObject(dc, old);
            SelectObject(dc, p);
            stroke(dc, &[(8, 12), (11, 15), (17, 9)], x, y, s, color, false);
        }
        "home" => stroke(dc, &[(3, 11), (12, 3), (21, 11), (19, 11), (19, 20), (5, 20), (5, 11), (3, 11)], x, y, s, color, true),
        "grid" => {
            for (ax, ay) in [(5, 5), (13, 5), (5, 13), (13, 13)] {
                stroke(dc, &[(ax, ay), (ax + 6, ay), (ax + 6, ay + 6), (ax, ay + 6)], x, y, s, color, true);
            }
        }
        "list" => {
            for (a, b) in [(5, 7), (5, 12), (5, 17)] {
                stroke(dc, &[(a, b), (20, b)], x, y, s, color, false);
            }
        }
        "palette" => {
            let old = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = SelectObject(dc, style::pen(color, 2 * sc()) as _);
            Ellipse(dc, x + s * 3 / 24, y + s * 3 / 24, x + s * 21 / 24, y + s * 21 / 24);
            SelectObject(dc, old);
            SelectObject(dc, p);
            for (a, b) in [(8, 10), (12, 8), (16, 11)] {
                circle(dc, x + s * (a - 1) / 24, y + s * (b - 1) / 24, s / 5, color);
            }
        }
        _ => {}
    }
}

fn icon_for(id: u32, title: &str) -> &'static str {
    match id {
        1 => "download",
        2 | 207 => "folder",
        4 | 81 => "restart",
        5 => "pulse",
        7 => "speaker",
        10 => {
            if title.starts_with("Resume") {
                "play"
            } else {
                "pause"
            }
        }
        12 => "mic",
        14 => "gear",
        40 => "info",
        70 => "select",
        71 => "trash",
        73 => "headset",
        85 => "tag",
        89 => "palette",
        _ => "list",
    }
}

// ── Window chrome ────────────────────────────────────────────────────────────
/// Card bounds for the polished, slightly wider window. Uses the same rows as
/// the original layout; only the right edge follows the client width.
pub use crate::nav::{nav_page, set_nav_page, set_sidebar, sidebar, SB};

pub fn boxes(mode: usize, client_right: i32) -> Vec<(i32, i32, i32, i32)> {
    let right = (client_right - 24).max(300);
    if mode == 4 {
        // One page at a time: a single content card to the right of the rail.
        return vec![(SB + 16, 88, right, 622)];
    }
    match mode {
        0 => vec![(16, 88, 474, 622), (490, 88, right, 622)],
        1 => vec![(16, 88, 588, 356), (16, 372, 588, 590)],
        2 => vec![(16, 88, 380, 602), (396, 88, right, 602)],
        _ => vec![(16, 88, right, 334), (16, 350, right, 622)],
    }
}

/// Reference-style empty library illustration, painted in the card's free area.
pub unsafe fn empty_state(
    dc: HDC,
    card: (i32, i32, i32, i32),
    count: usize,
    theme_index: usize,
    accent_index: usize,
) {
    if !enabled() || count != 0 {
        return;
    }
    let (left, top, right, bottom) = card;
    if right - left < 220 || bottom - top < 260 {
        return;
    }
    let t = theme(theme_index, accent_index);
    let cx = (left + right) / 2;
    let cy = top + (bottom - top) / 2 - 10;
    let well = RECT {
        left: cx - 30,
        top: cy - 66,
        right: cx + 30,
        bottom: cy - 6,
    };
    style::rect(dc, well, style::mix(t.panel, t.accent, 13), style::mix(t.panel, t.accent, 13), 18);
    icon(dc, "bolt", well.left + 17, well.top + 17, 26, t.accent);
    let title = RECT {
        left: left + 20,
        top: cy + 8,
        right: right - 20,
        bottom: cy + 34,
    };
    style::label(dc, "No clips yet", title, font(18, 700), t.text, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    let hint = RECT {
        left: left + 20,
        top: cy + 36,
        right: right - 20,
        bottom: cy + 58,
    };
    style::label(
        dc,
        "Press Alt+G after a moment worth keeping.",
        hint,
        font(12, 400),
        t.muted,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

pub unsafe fn header(dc: HDC, client: RECT, settings: bool, theme_index: usize, accent_index: usize) {
    if !enabled() {
        return;
    }
    let t = theme(theme_index, accent_index);
    let side = sidebar() && !settings;
    if side {
        let rail = RECT { left: 0, top: 0, right: SB, bottom: client.bottom };
        FillRect(dc, &rail, style::brush(t.panel));
        let edge = RECT { left: SB - 1, top: 0, right: SB, bottom: client.bottom };
        FillRect(dc, &edge, style::brush(t.line));
        style::label(
            dc,
            &crate::identity::name(),
            RECT { left: 76, top: 20, right: SB - 8, bottom: 58 },
            font(16, 700),
            t.text,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );
    }
    if !settings {
        let r = RECT {
            left: 26,
            top: 18,
            right: 68,
            bottom: 60,
        };
        // Two concentric rounds give a soft raised tile without per-frame bitmaps.
        style::rect(dc, r, style::mix(t.accent, 0, 12), style::mix(t.accent, 0, 26), 13);
        style::rect(
            dc,
            RECT {
                left: r.left + 2,
                top: r.top + 1,
                right: r.right - 2,
                bottom: r.bottom - 3,
            },
            t.accent,
            t.accent,
            11,
        );
        brand_glyph(dc, r, t.ink);
    }
    let line = RECT {
        left: if side { SB + 24 } else { 24 },
        top: if settings { 78 } else { 84 },
        right: client.right - 24,
        bottom: if settings { 79 } else { 85 },
    };
    FillRect(dc, &line, style::brush(t.line));
}

// ── Owner-drawn controls ─────────────────────────────────────────────────────
unsafe fn label_button(
    d: &DRAWITEMSTRUCT,
    title: &str,
    icon_name: &str,
    primary: bool,
    ghost: bool,
    s: State,
    t: style::Theme,
) {
    let fill = if s.disabled {
        t.raise
    } else if primary {
        if s.pressed {
            style::mix(t.accent, t.ink, 15)
        } else {
            t.accent
        }
    } else if s.pressed || s.hover {
        t.hover
    } else if ghost {
        t.panel
    } else {
        t.raise
    };
    let edge = if primary || ghost {
        fill
    } else {
        t.border
    };
    style::rect(d.hDC, d.rcItem, fill, edge, 10);
    let fg = if s.disabled {
        t.dim
    } else if primary {
        t.ink
    } else {
        t.text
    };
    let h = d.rcItem.bottom - d.rcItem.top;
    let size = h.min(20);
    let icon_x = d.rcItem.left + 10;
    icon(
        d.hDC,
        icon_name,
        icon_x,
        d.rcItem.top + (h - size) / 2,
        size,
        fg,
    );
    // Reserve the icon column so short buttons never crowd their label.
    let r = RECT {
        left: icon_x + size + 8,
        top: d.rcItem.top,
        right: d.rcItem.right - 8,
        bottom: d.rcItem.bottom,
    };
    style::label(
        d.hDC,
        title,
        r,
        font(14, 600),
        fg,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    if s.focused {
        style::focus(d.hDC, d.rcItem, t);
    }
}

unsafe fn source_row(d: &DRAWITEMSTRUCT, title: &str, s: State, t: style::Theme, on: bool) {
    let (name, description) = title.split_once('\n').unwrap_or((title, ""));
    let id = d.CtlID;
    // In Settings the row title is already on the left, so the pill shows only the state
    // (the control keeps its full text as its accessible name).
    let in_settings = GetDlgCtrlID(GetParent(d.hwndItem)) == 74;
    let state_name;
    let name = if in_settings && (id == 7 || id == 12) {
        state_name = if name.ends_with("ON") { "On" } else { "Off" };
        state_name
    } else {
        name
    };
    let color = if id == 7 || id == 86 { OK } else if id == 12 || id == 87 { BLUE } else { VIOLET };
    let fill = if s.hover || s.focused {
        t.hover
    } else {
        style::mix(t.panel, t.text, 4)
    };
    style::rect(d.hDC, d.rcItem, fill, if s.focused { t.accent } else { t.panel }, 12);
    let h = d.rcItem.bottom - d.rcItem.top;
    let well = h.min(36) - 4;
    let wy = d.rcItem.top + (h - well) / 2;
    let wx = d.rcItem.left + 10;
    let narrow_row = d.rcItem.right - d.rcItem.left < crate::layout_plan::SOURCE_NARROW_W;
    if !narrow_row {
    style::rect(
        d.hDC,
        RECT {
            left: wx,
            top: wy,
            right: wx + well,
            bottom: wy + well,
        },
        style::mix(t.panel, color, 8),
        style::mix(t.panel, color, 8),
        8,
    );
    icon(
        d.hDC,
        icon_for(id, ""),
        wx + 5,
        wy + 5,
        well - 10,
        if on { color } else { t.dim },
    );
    }
    let row_w = d.rcItem.right - d.rcItem.left;
    let (text_l, right_m, name_px) = crate::layout_plan::source_text_cols(row_w);
    let narrow = row_w < crate::layout_plan::SOURCE_NARROW_W;
    let mut r = RECT {
        left: d.rcItem.left + text_l,
        top: d.rcItem.top + 6,
        right: d.rcItem.right - right_m,
        bottom: d.rcItem.bottom - 6,
    };
    if h < 44 || description.is_empty() || narrow {
        style::label(d.hDC, name, r, font(name_px, 600), t.text, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    } else {
        let mut top = r;
        top.bottom = top.top + 20;
        style::label(d.hDC, name, top, font(14, 600), t.text, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
        r.top = top.bottom;
        style::label(d.hDC, description, r, font(12, 400), t.muted, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    }
    // Static, passive level indicator: no animation timer while a game is running.
    if !narrow {
    let meter_right = d.rcItem.right - 58;
    let heights = [4, 11, 17, 9, 5];
    for (i, hh) in heights.iter().enumerate() {
        let mx = meter_right - 20 + i as i32 * 4;
        let my = d.rcItem.top + (h - 18) / 2 + (18 - *hh);
        style::rect(
            d.hDC,
            RECT {
                left: mx,
                top: my,
                right: mx + 3,
                bottom: my + *hh,
            },
            if on { color } else { t.dim },
            if on { color } else { t.dim },
            1,
        );
    }
    }
    style::switch_at(
        d.hDC,
        d.rcItem,
        if on { 1. } else { 0. },
        style::State {
            hover: s.hover,
            ..Default::default()
        },
        t,
    );
    if s.focused {
        style::focus(d.hDC, d.rcItem, t);
    }
}

/// The two halves of the library density control share one rounded container.
unsafe fn density_button(d: &DRAWITEMSTRUCT, glyph: &str, s: State, t: style::Theme) {
    let selected = prop(d.hwndItem, "density-selected") != 0;
    let fill = if selected {
        t.accent
    } else if s.hover || s.focused {
        t.hover
    } else {
        t.raise
    };
    let edge = if selected { t.accent } else { t.border };
    style::rect(d.hDC, d.rcItem, fill, edge, 8);
    let ink = if selected { t.ink } else { t.muted };
    let side = 18;
    let (w, h) = (d.rcItem.right - d.rcItem.left, d.rcItem.bottom - d.rcItem.top);
    icon(d.hDC, glyph, d.rcItem.left + (w - side) / 2, d.rcItem.top + (h - side) / 2, side, ink);
    if s.focused {
        style::focus(d.hDC, d.rcItem, t);
    }
}

/// Handles the replay controls. Returns false for controls Legacy mode should own.
pub unsafe fn draw_control(d: &DRAWITEMSTRUCT) -> bool {
    if !enabled() {
        return false;
    }
    let parent = GetParent(d.hwndItem);
    if prop(parent, "display-settings") != 0 || crate::display::font_name().is_empty() {
        return false;
    }
    let theme_index = crate::ui::theme_index();
    let accent_index = crate::ui::accent_index();
    let t = theme(theme_index, accent_index);
    let s = control_state(d);
    let title = text(d.hwndItem);
    let id = d.CtlID;
    if id == 7 || id == 12 || id == 73 || id == 86 || id == 87 {
        let on = id == 73 || prop(d.hwndItem, "source-on") != 0;
        source_row(d, &title, s, t, on);
        return true;
    }
    if (110..=113).contains(&id) {
        let page = nav_page();
        let on = (id == 110 && page == 0) || (id == 111 && page == 1);
        let (glyph, label) = match id {
            110 => ("home", "Home"),
            111 => ("play", "Clips"),
            112 => ("scissors", "Editor"),
            _ => ("gear", "Settings"),
        };
        let fill = if on {
            style::mix(t.panel, t.accent, 18)
        } else if s.hover || s.focused || s.pressed {
            t.hover
        } else {
            t.panel
        };
        style::rect(d.hDC, d.rcItem, fill, if on { t.accent } else { fill }, 10);
        let ink = if on { t.accent } else { t.muted };
        let h = d.rcItem.bottom - d.rcItem.top;
        let w = d.rcItem.right - d.rcItem.left;
        if w < 90 {
            // Icon-only page button (Compact layout); the name is its tooltip.
            icon(d.hDC, glyph, d.rcItem.left + (w - 18) / 2, d.rcItem.top + (h - 18) / 2, 18, ink);
        } else {
            icon(d.hDC, glyph, d.rcItem.left + 12, d.rcItem.top + (h - 18) / 2, 18, ink);
            let mut r = d.rcItem;
            r.left += 40;
            style::label(d.hDC, label, r, font(14, if on { 700 } else { 500 }), if on { t.text } else { t.muted }, DT_SINGLELINE | DT_VCENTER);
        }
        if s.focused {
            style::focus(d.hDC, d.rcItem, t);
        }
        return true;
    }
    if id == 125 {
        // "View all": a text link at the right of the Recent clips heading.
        FillRect(d.hDC, &d.rcItem, style::brush(crate::ui::backdrop(d.hwndItem, d.rcItem)));
        let ink = if s.hover || s.focused { t.text } else { t.accent };
        style::label(d.hDC, "View all", d.rcItem, font(13, 600), ink, DT_SINGLELINE | DT_VCENTER | DT_RIGHT);
        if s.focused {
            style::focus(d.hDC, d.rcItem, t);
        }
        return true;
    }
    if id == 114 {
        // Icon-only; the window text and tooltip keep the name.
        density_button(d, "grid", s, t);
        return true;
    }
    if id == 36 || id == 90 {
        // Large is the 2x2 squares icon, Compact is the rows icon; the window text keeps the names.
        density_button(d, if id == 90 { "grid" } else { "list" }, s, t);
        return true;
    }
    match id {
        1 => {
            let (caption, key) = title.split_once("   ").unwrap_or((&title, ""));
            let fill = if s.disabled {
                t.raise
            } else if s.pressed {
                style::mix(t.accent, t.ink, 15)
            } else {
                t.accent
            };
            style::rect(d.hDC, d.rcItem, fill, fill, 14);
            let fg = if s.disabled { t.dim } else { t.ink };
            let h = d.rcItem.bottom - d.rcItem.top;
            icon(d.hDC, "download", d.rcItem.left + 14, d.rcItem.top + (h - 20) / 2, 20, fg);
            let mut label_rect = d.rcItem;
            label_rect.left += 40;
            label_rect.right -= if key.is_empty() { 16 } else { 100 };
            let caption = if s.disabled { "Waiting for capture..." } else { caption };
            style::label(d.hDC, caption, label_rect, font(15, 700), fg, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
            if !key.is_empty() {
                let cap = RECT {
                    left: d.rcItem.right - 90,
                    top: d.rcItem.top + (h - 24) / 2,
                    right: d.rcItem.right - 14,
                    bottom: d.rcItem.top + (h - 24) / 2 + 24,
                };
                style::keycap(d.hDC, cap, key, t, font(12, 700));
            }
            if s.focused {
                style::focus(d.hDC, d.rcItem, t);
            }
            true
        }
        43 => {
            let mut state = s;
            state.pressed = s.pressed || title.starts_with("Stop recording");
            let name = if d.rcItem.right - d.rcItem.left < 180 {
                if title.starts_with("Stop recording") { "Stop" } else { "Record" }
            } else {
                &title
            };
            label_button(d, name, "bolt", false, false, state, t);
            true
        }
        2 | 4 | 5 | 10 | 40 | 70 | 71 | 81 | 85 | 89 => {
            let compact = (id == 10 || id == 43 || id == 71) && d.rcItem.right - d.rcItem.left < 180;
            let name = if compact && id == 10 {
                if title.starts_with("Resume") { "Resume" } else { "Pause" }
            } else if compact && id == 71 {
                "Delete"
            } else {
                title.as_str()
            };
            label_button(d, name, icon_for(id, &title), false, false, s, t);
            true
        }
        _ => false,
    }
}

// ── Static panels ────────────────────────────────────────────────────────────
unsafe fn number_pair(h: HWND) -> (usize, usize) {
    let value = text(h);
    value
        .split_once('/')
        .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
        .unwrap_or((0, 30))
}

pub unsafe fn draw_buffer(h: HWND, dc: HDC, t: style::Theme) -> bool {
    if !enabled() || prop(h, "replay-buffer") == 0 {
        return false;
    }
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let (filled, total) = number_pair(h);
    let n = total.clamp(1, 45);
    let gap = 3;
    let w = ((r.right - gap * (n - 1) as i32) / n as i32).max(2);
    for i in 0..n {
        let on = i * total / n < filled;
        let color = if on {
            // Opacity ramp from the reference: older frames are dimmer.
            style::mix(t.panel, t.accent, 32 + (66 * i / n.max(1)) as u32)
        } else {
            style::mix(t.panel, t.border, 65)
        };
        let x = i as i32 * (w + gap);
        style::rect(dc, RECT { left: x, top: 1, right: x + w, bottom: 21 }, color, color, 2);
    }
    let f = font(12, 400);
    style::label(dc, &format!("{total}s ago"), RECT { left: 0, top: 25, right: r.right / 2, bottom: r.bottom }, f, t.muted, DT_SINGLELINE | DT_VCENTER);
    style::label(dc, "now", RECT { left: r.right / 2, top: 25, right: r.right, bottom: r.bottom }, f, t.muted, DT_SINGLELINE | DT_VCENTER | DT_RIGHT);
    true
}

pub unsafe fn draw_note(h: HWND, dc: HDC, t: style::Theme) -> bool {
    if !enabled() || prop(h, "replay-note") == 0 || prop(GetParent(h), "display-settings") != 0 {
        return false;
    }
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    style::rect(dc, r, style::mix(t.panel, WARN, 10), style::mix(t.panel, WARN, 30), 10);
    let height = r.bottom - r.top;
    let dot = 7.min(height / 3);
    let cy = r.top + (height - dot) / 2;
    style::disc(dc, RECT { left: r.left + 11, top: cy, right: r.left + 11 + dot, bottom: cy + dot }, WARN, None);
    let mut tr = r;
    tr.left += 26;
    tr.right -= 10;
    let value = text(h);
    let note = if r.right < 380 {
        "Source changes clear the unsaved replay."
    } else {
        &value
    };
    style::label(dc, note, tr, font(12, 400), t.muted, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    true
}

/// True when the library hint window is visible and will paint the empty illustration itself
/// (so the card-level one must not also paint, or two bolts show at different heights).
pub unsafe fn hint_paints_empty(parent: HWND) -> bool {
    let hint = windows_sys::Win32::UI::WindowsAndMessaging::GetPropW(
        parent,
        "library-hint\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
    ) as HWND;
    if hint.is_null()
        || windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hint) == 0
        || prop(hint, "polished-empty") == 0
    {
        return false;
    }
    let mut r = RECT::default();
    windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hint, &mut r);
    r.right >= 220 && r.bottom >= 90
}

/// Painted inside the library hint window (which otherwise covers the card).
pub unsafe fn draw_empty(h: HWND, dc: HDC, t: style::Theme) -> bool {
    if !enabled() || prop(h, "polished-empty") == 0 {
        return false;
    }
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    if r.right < 220 || r.bottom < 90 {
        return false;
    }
    FillRect(dc, &r, style::brush(t.panel));
    let cx = r.right / 2;
    let cy = r.bottom / 2;
    let well = RECT {
        left: cx - 28,
        top: cy - 62,
        right: cx + 28,
        bottom: cy - 6,
    };
    style::rect(dc, well, style::mix(t.panel, t.accent, 13), style::mix(t.panel, t.accent, 13), 18);
    icon(dc, "bolt", well.left + 16, well.top + 16, 24, t.accent);
    style::label(
        dc,
        "No clips yet",
        RECT {
            left: 0,
            top: cy + 4,
            right: r.right,
            bottom: cy + 32,
        },
        font(18, 700),
        t.text,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    style::label(
        dc,
        "Press Alt+G after a moment worth keeping.",
        RECT {
            left: 0,
            top: cy + 34,
            right: r.right,
            bottom: cy + 56,
        },
        font(12, 400),
        t.muted,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    true
}

pub unsafe fn draw_status(h: HWND, dc: HDC, t: style::Theme) -> bool {
    if !enabled() || prop(h, "replay-status") == 0 || prop(GetParent(h), "display-settings") != 0 {
        return false;
    }
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let value = text(h);
    let color = if value.contains("error")
        || value.contains("failed")
        || value.contains("Recording")
        || value.contains("Stop")
    {
        REC
    } else if value.contains("paused") {
        WARN
    } else {
        t.accent
    };
    // The control buffer starts black; paint the card colour first so the rounded corners are clean.
    FillRect(dc, &r, style::brush(t.panel));
    style::rect(dc, r, style::mix(t.panel, color, 12), style::mix(t.panel, color, 12), 12);
    let h = r.bottom - r.top;
    let dot = 7.min(h / 3);
    let cy = r.top + (h - dot) / 2;
    style::disc(dc, RECT { left: r.left + 10, top: cy, right: r.left + 10 + dot, bottom: cy + dot }, color, None);
    let mut tr = r;
    tr.left += 25;
    tr.right -= 8;
    style::label(dc, &value, tr, font(12, 600), color, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    true
}

/// Settings header Back button, drawn with a back glyph and icon-led label.
pub unsafe fn draw_back(d: &DRAWITEMSTRUCT) -> bool {
    if !enabled() || d.CtlID != 14 || prop(GetParent(d.hwndItem), "display-settings") == 0 {
        return false;
    }
    let t = theme(crate::ui::theme_index(), crate::ui::accent_index());
    let s = control_state(d);
    style::rect(d.hDC, d.rcItem, if s.hover { t.hover } else { t.panel }, t.panel, 10);
    icon(d.hDC, "back", d.rcItem.left + 10, d.rcItem.top + 8, 20, t.text);
    let mut r = d.rcItem;
    r.left += 32;
    style::label(d.hDC, "Back to replay", r, font(14, 600), t.text, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    if s.focused {
        style::focus(d.hDC, d.rcItem, t);
    }
    true
}

/// Client rectangle of the three section buttons, in the parent's coordinates.
pub unsafe fn pill_band(parent: HWND) -> RECT {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut u = RECT { left: i32::MAX, top: i32::MAX, right: i32::MIN, bottom: i32::MIN };
    for id in 82..=84 {
        let mut r = RECT::default();
        GetWindowRect(GetDlgItem(parent, id), &mut r);
        let mut p = [windows_sys::Win32::Foundation::POINT { x: r.left, y: r.top }, windows_sys::Win32::Foundation::POINT { x: r.right, y: r.bottom }];
        windows_sys::Win32::Graphics::Gdi::MapWindowPoints(std::ptr::null_mut(), parent, p.as_mut_ptr(), 2);
        u.left = u.left.min(p[0].x);
        u.top = u.top.min(p[0].y);
        u.right = u.right.max(p[1].x);
        u.bottom = u.bottom.max(p[1].y);
    }
    u
}
/// While the Settings pill slides, paint the whole pill on the parent. The buttons cover their
/// own part with the same shape, so only the gaps between them show this; no seam remains.
pub unsafe fn pill_backdrop(dc: HDC, parent: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    if !enabled() {
        return;
    }
    let (pos, moving) = crate::ui::motion::pill();
    if !moving {
        return;
    }
    let t = theme(crate::ui::theme_index(), crate::ui::accent_index());
    let (mut a, mut b) = (RECT::default(), RECT::default());
    GetWindowRect(GetDlgItem(parent, 82), &mut a);
    GetWindowRect(GetDlgItem(parent, 83), &mut b);
    let mut p = [windows_sys::Win32::Foundation::POINT { x: a.left, y: a.top }];
    windows_sys::Win32::Graphics::Gdi::MapWindowPoints(std::ptr::null_mut(), parent, p.as_mut_ptr(), 1);
    let (dx, dy) = ((b.left - a.left) as f32, (b.top - a.top) as f32);
    let (ox, oy) = ((pos * dx).round() as i32, (pos * dy).round() as i32);
    let w = a.right - a.left;
    let h = a.bottom - a.top;
    let r = RECT { left: p[0].x + ox, top: p[0].y + oy, right: p[0].x + ox + w, bottom: p[0].y + oy + h };
    style::rect(dc, r, t.raise, t.panel, 10);
    FillRect(dc, &RECT { left: r.left + 4, top: r.top + 9, right: r.left + 7, bottom: r.bottom - 9 }, style::brush(t.accent));
}

pub unsafe fn draw_section(d: &DRAWITEMSTRUCT) -> bool {
    if !enabled() || !(82..=84).contains(&d.CtlID) {
        return false;
    }
    let t = theme(crate::ui::theme_index(), crate::ui::accent_index());
    let selected = prop(d.hwndItem, "section-active") != 0;
    let s = control_state(d);
    let (pos, moving) = crate::ui::motion::pill();
    let index = (d.CtlID - 82) as f32;
    // While the pill slides, each button paints the slice of it that overlaps its own area.
    let slide = if moving {
        let parent = windows_sys::Win32::UI::WindowsAndMessaging::GetParent(d.hwndItem);
        let (mut a, mut b) = (RECT::default(), RECT::default());
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem(parent, 82), &mut a);
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem(parent, 83), &mut b);
        let (dx, dy) = ((b.left - a.left) as f32, (b.top - a.top) as f32);
        Some((((pos - index) * dx).round() as i32, ((pos - index) * dy).round() as i32))
    } else {
        None
    };
    if s.hover && !selected {
        style::rect(d.hDC, d.rcItem, t.hover, t.panel, 10);
    }
    if let Some((ox, oy)) = slide {
        if (pos - index).abs() < 1.0 {
            let r = RECT {
                left: d.rcItem.left + ox,
                top: d.rcItem.top + oy,
                right: d.rcItem.right + ox,
                bottom: d.rcItem.bottom + oy,
            };
            style::rect(d.hDC, r, t.raise, t.panel, 10);
            FillRect(
                d.hDC,
                &RECT { left: r.left + 4, top: r.top + 9, right: r.left + 7, bottom: r.bottom - 9 },
                style::brush(t.accent),
            );
        }
    } else if selected {
        style::rect(d.hDC, d.rcItem, t.raise, t.panel, 10);
    }
    let glyph = match d.CtlID {
        82 => "video",
        83 => "palette",
        _ => "folder",
    };
    let name = match d.CtlID {
        82 => "Capture",
        83 => "Appearance",
        _ => "Files / app",
    };
    icon(d.hDC, glyph, d.rcItem.left + 10, d.rcItem.top + 11, 18, if selected { t.accent } else { t.muted });
    let mut r = d.rcItem;
    r.left += 34;
    style::label(d.hDC, name, r, font(14, if selected { 600 } else { 400 }), if selected { t.text } else { t.muted }, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    if selected && slide.is_none() {
        FillRect(
            d.hDC,
            &RECT {
                left: d.rcItem.left + 4,
                top: d.rcItem.top + 9,
                right: d.rcItem.left + 7,
                bottom: d.rcItem.bottom - 9,
            },
            style::brush(t.accent),
        );
    }
    if s.focused {
        style::focus(d.hDC, d.rcItem, t);
    }
    true
}

// ── Home real-state panel (Sidebar Home, wide window) ────────────────────────
/// Three stat cards and the Recent clips heading, from `home_stats::info()`. The recent
/// cards themselves are real controls (thumbnails, click to open). A card with no data yet
/// is skipped rather than filled with a placeholder.
pub unsafe fn home_panel(dc: HDC, client_right: i32, theme_index: usize, accent_index: usize) {
    if !enabled() {
        return;
    }
    let t = theme(theme_index, accent_index);
    let info = crate::home_stats::info();
    let (x, w) = crate::nav::home_panel(client_right);
    let stack = crate::nav::panel_stack(w);
    let gap = 10;
    let cw = (w - 2 * gap) / 3;
    let cards = [
        ("REPLAY", info.replay.clone(), String::from("rolling buffer"), info.have_capture),
        ("CAPTURE", info.capture.clone(), info.capture_sub.clone(), info.have_capture),
        ("LIBRARY", info.library.clone(), info.library_sub.clone(), info.have_library),
    ];
    for (i, (name, value, sub, have)) in cards.iter().enumerate() {
        if !*have {
            continue;
        }
        if stack {
            let top = 104 + i as i32 * 42;
            let r = RECT { left: x, top, right: x + w, bottom: top + 36 };
            style::rect(dc, r, t.raise, t.border, 10);
            style::label(dc, name, RECT { left: r.left + 12, top: r.top, right: r.left + 84, bottom: r.bottom }, font(11, 600), t.dim, DT_SINGLELINE | DT_VCENTER);
            let line = format!("{value}  {sub}");
            style::label(dc, &line, RECT { left: r.left + 88, top: r.top, right: r.right - 10, bottom: r.bottom }, font(13, 600), t.text, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
            continue;
        }
        let left = x + i as i32 * (cw + gap);
        let r = RECT { left, top: 104, right: left + cw, bottom: 192 };
        style::rect(dc, r, t.raise, t.border, 12);
        let line = |top: i32, bottom: i32| RECT { left: r.left + 12, top, right: r.right - 8, bottom };
        style::label(dc, name, line(r.top + 10, r.top + 26), font(11, 600), t.dim, DT_SINGLELINE | DT_VCENTER);
        style::label(dc, value, line(r.top + 30, r.top + 54), font(16, 700), t.text, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
        style::label(dc, sub, line(r.top + 58, r.top + 76), font(12, 400), t.muted, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    }
    if !info.have_library {
        return;
    }
    let hy = crate::nav::panel_heading_y(w);
    style::label(dc, "Recent clips", RECT { left: x, top: hy, right: x + w - 90, bottom: hy + 24 }, font(14, 700), t.text, DT_SINGLELINE | DT_VCENTER);
    if info.recent.is_empty() {
        style::label(dc, "No clips yet. Saved replays appear here.", RECT { left: x, top: hy + 32, right: x + w, bottom: hy + 56 }, font(13, 400), t.muted, DT_SINGLELINE | DT_VCENTER);
    }
}
