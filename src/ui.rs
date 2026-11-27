use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
fn images() -> &'static Mutex<BTreeMap<PathBuf, usize>> {
    static V: OnceLock<Mutex<BTreeMap<PathBuf, usize>>> = OnceLock::new();
    V.get_or_init(Default::default)
}
fn rows() -> &'static Mutex<BTreeMap<usize, PathBuf>> {
    static V: OnceLock<Mutex<BTreeMap<usize, PathBuf>>> = OnceLock::new();
    V.get_or_init(Default::default)
}
pub fn store_thumb(path: PathBuf, bitmap: usize) {
    unsafe {
        let mut cache = images().lock().unwrap();
        if let Some(old) = cache.insert(path.clone(), bitmap) {
            DeleteObject(old as _);
        }
        while cache.len() > 10 {
            let key = cache.keys().find(|k| **k != path).unwrap().clone();
            if let Some(old) = cache.remove(&key) {
                DeleteObject(old as _);
            }
        }
    }
}
fn badges() -> &'static Mutex<BTreeMap<usize, Vec<(String, u32)>>> {
    static V: OnceLock<Mutex<BTreeMap<usize, Vec<(String, u32)>>>> = OnceLock::new();
    V.get_or_init(Default::default)
}
pub fn bind_thumb(row: HWND, path: PathBuf) {
    let data = path
        .parent()
        .map(|dir| {
            let tags = crate::catalog::tags(dir);
            let palette = crate::catalog::reusable(dir).unwrap_or_default();
            tags.get(
                &path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            )
            .map(|s| {
                s.split(',')
                    .map(|s| {
                        let t = s.trim().to_string();
                        let c = palette
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(&t))
                            .map(|(_, c)| *c)
                            .unwrap_or(crate::catalog::palette()[0]);
                        (t, c)
                    })
                    .collect()
            })
            .unwrap_or_default()
        })
        .unwrap_or_default();
    badges().lock().unwrap().insert(row as usize, data);
    rows().lock().unwrap().insert(row as usize, path);
}

use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::*,
        Input::KeyboardAndMouse::{
            GetCapture, GetFocus, IsWindowEnabled, ReleaseCapture, SetCapture, TME_LEAVE,
            TRACKMOUSEEVENT,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
static THEME: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub fn set_theme(i: usize) {
    THEME.store(i % 7, std::sync::atomic::Ordering::Relaxed);
    #[cfg(windows)]
    unsafe {
        style::warm(style_theme(), &crate::display::font_name(), 1.);
    }
}
pub fn theme_colors(i: usize) -> [u32; 5] {
    if i % 7 == crate::custom_theme::THEME_INDEX {
        return crate::custom_theme::palette(crate::custom_theme::current());
    }
    if crate::polished::enabled() {
        crate::polished::palette(i)
    } else {
        [
            [0x171310, 0x211d19, 0x39332b, 0xf4f0e8, 0xaba396],
            [0x0e0808, 0x201411, 0x50342a, 0xfff4e8, 0xbeb0a0],
            [0x160e19, 0x291d2c, 0x57415b, 0xffedfa, 0xc5a9c9],
            [0x0a1117, 0x15212c, 0x31485b, 0xe9f4ff, 0xa0b8cd],
            [0x0c1711, 0x19281d, 0x3d5341, 0xecfff0, 0xabc6b0],
            [0x080808, 0x151515, 0x666666, 0xffffff, 0xd0d0d0],
        ][i % 6]
    }
}
pub fn theme_index() -> usize {
    THEME.load(std::sync::atomic::Ordering::Relaxed)
}
pub fn accent_index() -> usize {
    ACCENT_INDEX.load(std::sync::atomic::Ordering::Relaxed)
}
fn palette() -> [u32; 5] {
    theme_colors(THEME.load(std::sync::atomic::Ordering::Relaxed))
}
pub fn bg() -> u32 {
    palette()[0]
}
pub fn panel() -> u32 {
    palette()[1]
}
pub fn border() -> u32 {
    palette()[2]
}
pub fn text_color() -> u32 {
    palette()[3]
}
pub fn muted() -> u32 {
    palette()[4]
}
fn blend(a: u32, b: u32) -> u32 {
    let mut c = 0;
    for i in [0, 8, 16] {
        c |= ((((a >> i) & 255) * 7 + ((b >> i) & 255)) / 8) << i;
    }
    c
}

static ACCENT_INDEX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub fn set_accent(i: usize) {
    ACCENT_INDEX.store(i % 8, std::sync::atomic::Ordering::Relaxed);
    #[cfg(windows)]
    unsafe {
        style::warm(style_theme(), &crate::display::font_name(), 1.);
    }
}
pub fn accent_colors() -> [u32; 8] {
    if crate::polished::enabled() {
        crate::polished::ACCENTS
    } else {
        [
            0xc6e89b, 0xf4c38d, 0xe8abcf, 0x84c6ed, 0xdbc06b, 0xa7d5ff, 0xe1c2ed, 0xf3f3f3,
        ]
    }
}
pub fn accent() -> u32 {
    accent_colors()[ACCENT_INDEX.load(std::sync::atomic::Ordering::Relaxed)]
}
pub unsafe extern "system" fn invalidate_child(h: HWND, _: isize) -> i32 {
    InvalidateRect(h, std::ptr::null(), 0);
    1
}

pub unsafe fn rounded(dc: HDC, r: RECT, fill: u32, edge: u32, radius: i32) {
    // GDI RoundRect takes the corner ellipse width; style::rect takes the radius.
    style::rect(dc, r, fill, edge, radius / 2);
}
pub unsafe fn handle(hwnd: HWND, msg: u32, w: usize, l: isize) -> Option<isize> {
    if msg == WM_DRAWITEM {
        let original = &*(l as *const DRAWITEMSTRUCT);
        let mut d = std::ptr::read(original);
        let width = d.rcItem.right.max(1);
        let height = d.rcItem.bottom.max(1);
        let mem = CreateCompatibleDC(d.hDC);
        let bitmap = CreateCompatibleBitmap(d.hDC, width, height);
        let old = SelectObject(mem, bitmap);
        let screen = d.hDC;
        d.hDC = mem;
        // A new bitmap is black. Seed it with the backdrop the control sits on so the corners
        // outside owner-drawn rounded shapes show the parent, not black.
        {
            let back = backdrop(d.hwndItem, d.rcItem);
            let b = CreateSolidBrush(back);
            FillRect(mem, &RECT { left: 0, top: 0, right: width, bottom: height }, b);
            DeleteObject(b);
        }
        let result = handle_inner(hwnd, msg, w, &d as *const _ as isize);
        BitBlt(
            screen,
            d.rcItem.left,
            d.rcItem.top,
            d.rcItem.right - d.rcItem.left,
            d.rcItem.bottom - d.rcItem.top,
            mem,
            d.rcItem.left,
            d.rcItem.top,
            SRCCOPY,
        );
        SelectObject(mem, old);
        DeleteObject(bitmap);
        DeleteDC(mem);
        return result;
    }
    handle_inner(hwnd, msg, w, l)
}
/// Colour of the parent just outside the control's four corners (most common wins).
/// Falls back to the theme background when the parent has not painted there yet.
pub(crate) unsafe fn backdrop(item: HWND, rc: RECT) -> u32 {
    let fallback = bg();
    let parent = GetParent(item);
    if parent.is_null() {
        return fallback;
    }
    let mut origin = POINT { x: 0, y: 0 };
    MapWindowPoints(item, parent, &mut origin, 1);
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    // Main window: the backing colour is known from the same layout the paint uses.
    let main_window = !GetPropW(parent, "display-ready\0".encode_utf16().collect::<Vec<_>>().as_ptr()).is_null();
    if main_window {
        let settings = !GetPropW(parent, "display-settings\0".encode_utf16().collect::<Vec<_>>().as_ptr()).is_null();
        if settings {
            return bg();
        }
        let layout = GetPropW(parent, "display-layout\0".encode_utf16().collect::<Vec<_>>().as_ptr()) as usize;
        if crate::polished::enabled() && crate::polished::sidebar() && origin.x + w <= crate::polished::SB {
            return panel();
        }
        let mut all = RECT::default();
        GetClientRect(parent, &mut all);
        let boxes = if crate::polished::enabled() {
            crate::polished::boxes(layout, all.right)
        } else {
            crate::replay_design::boxes_for(layout, false, all.bottom)
        };
        let mut boxes = boxes;
        crate::layout_plan::clamp_cards(&mut boxes, all.bottom);
        let (cx, cy) = (origin.x + w / 2, origin.y + h / 2);
        if boxes.iter().any(|b| cx >= b.0 && cx < b.2 && cy >= b.1 && cy < b.3) {
            return panel();
        }
        return bg();
    }
    let (l, t) = (origin.x - 2, origin.y - 2);
    let pts = [(l, t), (l + w + 3, t), (l, t + h + 3), (l + w + 3, t + h + 3)];
    let dc = GetDC(parent);
    if dc.is_null() {
        return fallback;
    }
    let mut seen: Vec<u32> = Vec::new();
    for (x, y) in pts {
        let c = GetPixel(dc, x, y);
        if c != 0xffff_ffff && c != 0 {
            seen.push(c);
        }
    }
    ReleaseDC(parent, dc);
    let mut best = fallback;
    let mut best_n = 0;
    for c in &seen {
        let n = seen.iter().filter(|x| *x == c).count();
        if n > best_n {
            best = *c;
            best_n = n;
        }
    }
    best
}
unsafe fn handle_inner(hwnd: HWND, msg: u32, w: usize, l: isize) -> Option<isize> {
    match msg {
        WM_MEASUREITEM => {
            let item = &mut *(l as *mut MEASUREITEMSTRUCT);
            if item.CtlType == ODT_COMBOBOX {
                item.itemHeight = 34;
                return Some(1);
            }
            None
        }
        WM_CTLCOLOREDIT => {
            SetBkColor(w as _, panel());
            SetTextColor(w as _, text_color());
            static B: OnceLock<Mutex<BTreeMap<u32, usize>>> = OnceLock::new();
            let mut b = B.get_or_init(Default::default).lock().unwrap();
            Some(
                *b.entry(panel())
                    .or_insert_with(|| CreateSolidBrush(panel()) as usize) as isize,
            )
        }
        WM_CTLCOLORBTN => {
            SetBkColor(w as _, panel());
            SetTextColor(w as _, text_color());
            static BR: OnceLock<Mutex<BTreeMap<u32, usize>>> = OnceLock::new();
            let mut b = BR.get_or_init(Default::default).lock().unwrap();
            Some(
                *b.entry(panel())
                    .or_insert_with(|| CreateSolidBrush(panel()) as usize) as isize,
            )
        }
        WM_CTLCOLORLISTBOX => {
            SetBkColor(w as _, panel());
            SetTextColor(w as _, text_color());
            static B: OnceLock<Mutex<BTreeMap<u32, usize>>> = OnceLock::new();
            let mut b = B.get_or_init(Default::default).lock().unwrap();
            Some(
                *b.entry(panel())
                    .or_insert_with(|| CreateSolidBrush(panel()) as usize) as isize,
            )
        }
        WM_CTLCOLORSTATIC => {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(l as HWND, &mut r);
            let mut pos = POINT {
                x: r.left,
                y: r.top,
            };
            ScreenToClient(hwnd, &mut pos);
            let layout = GetPropW(
                hwnd,
                "display-layout\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            ) as usize;
            let settings = GetPropW(
                hwnd,
                "display-settings\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            ) as usize
                != 0;
            let in_panel = if settings {
                false
            } else if layout == 3 {
                (100..604).contains(&pos.y)
            } else {
                (100..604).contains(&pos.y)
            };
            let color = if !GetPropW(
                hwnd,
                "surface-bg\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
            )
            .is_null()
            {
                bg()
            } else if in_panel {
                panel()
            } else {
                bg()
            };
            SetBkMode(w as _, OPAQUE as i32);
            SetBkColor(w as _, color);
            SetTextColor(w as _, text_color());
            static BRUSHES: OnceLock<Mutex<BTreeMap<u32, usize>>> = OnceLock::new();
            let mut brushes = BRUSHES.get_or_init(Default::default).lock().unwrap();
            Some(
                *brushes
                    .entry(color)
                    .or_insert_with(|| CreateSolidBrush(color) as usize) as isize,
            )
        }
        WM_ERASEBKGND => Some(1),
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let (dc, buffer) = begin_buffer(hwnd, &mut ps);
            let mut all: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut all);
            let b = CreateSolidBrush(bg());
            FillRect(dc, &all, b);
            DeleteObject(b);
            let in_settings = !GetPropW(
                hwnd,
                "display-settings\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
            .is_null();
            crate::polished::header(dc, all, in_settings, theme_index(), accent_index());
            if !in_settings && !crate::polished::enabled() {
                draw_identity_icon(dc, 26, 26, 30);
            }
            let layout = GetPropW(
                hwnd,
                "display-layout\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            ) as usize;
            let settings = GetPropW(
                hwnd,
                "display-settings\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            ) as usize
                != 0;
            if settings {
                FillRect(
                    dc,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: all.right,
                        bottom: 80,
                    },
                    style::brush(bg()),
                );
                crate::polished::pill_backdrop(dc, hwnd);
                let side = if all.right < 720 { 160 } else { 208 };
                style::rect(
                    dc,
                    RECT {
                        left: 0,
                        top: 80,
                        right: all.right,
                        bottom: 81,
                    },
                    border(),
                    border(),
                    0,
                );
                style::rect(
                    dc,
                    RECT {
                        left: side,
                        top: 80,
                        right: side + 1,
                        bottom: all.bottom,
                    },
                    border(),
                    border(),
                    0,
                );
                if all.right > 760 && crate::settings_view::current_category() == 1 {
                    // Appearance page only. Leave room for the interface-style button at the right.
                    // Stop left of the interface-style button, wherever it really is.
                    let mut pill_right = if crate::polished::enabled() { all.right - 306 } else { all.right - 20 };
                    for id in [89] {
                        let b = GetDlgItem(hwnd, id);
                        if !b.is_null() && IsWindowVisible(b) != 0 {
                            let mut br = RECT::default();
                            GetWindowRect(b, &mut br);
                            let mut p = POINT { x: br.left, y: br.top };
                            ScreenToClient(hwnd, &mut p);
                            pill_right = pill_right.min(p.x - 14);
                        }
                    }
                    let pill_left = (pill_right - 380).max(300);
                    if pill_right - pill_left >= 120 {
                    style::rect(
                        dc,
                        RECT {
                            left: pill_left,
                            top: 24,
                            right: pill_right,
                            bottom: 56,
                        },
                        style_theme().raise,
                        border(),
                        28,
                    );
                    style::label(
                        dc,
                        "Appearance applies instantly. Capture keeps running.",
                        RECT {
                            left: pill_left + 12,
                            top: 26,
                            right: pill_right - 4,
                            bottom: 54,
                        },
                        style::font(&crate::display::font_name(), 13, 400),
                        muted(),
                        DT_SINGLELINE | DT_CENTER | DT_VCENTER,
                    );
                    }
                }
            }
            let boxes = if crate::polished::enabled() && !settings {
                if layout == 1 || layout == 3 {
                    // Compact and Stacked: one card per page (Home capture controls, or the Clips library).
                    let right = if layout == 1 { 588 } else { crate::polished::boxes(3, all.right)[0].2 };
                    let clips = crate::polished::nav_page() == 1;
                    let bottom = match (layout, clips) {
                        (_, true) => (all.bottom - 100).max(300),
                        (1, false) => 540,
                        _ => 372,
                    };
                    vec![(16, 88, right, bottom)]
                } else {
                    crate::polished::boxes(layout, all.right)
                }
            } else {
                let mut client = RECT::default();
                GetClientRect(hwnd, &mut client);
                crate::replay_design::boxes_for(layout, settings, client.bottom)
            };
            let mut boxes = boxes;
            crate::layout_plan::clamp_cards(&mut boxes, all.bottom);
            let empty_card = if crate::polished::enabled() && !settings && (layout == 4 || layout == 1 || layout == 3) {
                if crate::polished::nav_page() == 1 { boxes.first().copied() } else { None }
            } else if crate::polished::enabled() && !settings && layout != 1 {
                boxes.get(1).copied()
            } else {
                None
            };
            for (left, top, right, bottom) in boxes {
                rounded(
                    dc,
                    RECT {
                        left,
                        top,
                        right,
                        bottom,
                    },
                    panel(),
                    border(),
                    24,
                );
            }
            if crate::polished::enabled()
                && !settings
                && layout == 4
                && crate::polished::nav_page() == 0
                && crate::nav::home_wide(all.right)
            {
                crate::polished::home_panel(dc, all.right, theme_index(), accent_index());
            }
            if let Some(card) = empty_card.filter(|_| !crate::polished::hint_paints_empty(hwnd)) {
                crate::polished::empty_state(
                    dc,
                    card,
                    {
                        // With a search, tag or favourites filter active, an empty list is "no
                        // matches", never the "No clips yet" illustration.
                        let f = crate::catalog::filter().lock().unwrap();
                        if !f.query.trim().is_empty() || !f.tag.is_empty() || f.favourites {
                            1
                        } else {
                            GetPropW(hwnd, "library-count\0".encode_utf16().collect::<Vec<_>>().as_ptr()) as usize
                        }
                    },
                    theme_index(),
                    accent_index(),
                );
            }
            let pen = CreatePen(PS_SOLID, 1, border());
            let old = SelectObject(dc, pen);

            SelectObject(dc, old);
            DeleteObject(pen);
            end_buffer(hwnd, &ps, dc, buffer);
            Some(0)
        }
        WM_DRAWITEM => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            if crate::polished::draw_back(d) {
                return Some(1);
            }
            if crate::polished::draw_section(d) {
                return Some(1);
            }
            if (100..=104).contains(&d.CtlID) || (120..=123).contains(&d.CtlID) {
                let path = rows().lock().unwrap().get(&(d.hwndItem as usize)).cloned();
                let bitmap = path
                    .as_ref()
                    .and_then(|p| images().lock().unwrap().get(p).copied());
                let tags = badges()
                    .lock()
                    .unwrap()
                    .get(&(d.hwndItem as usize))
                    .cloned()
                    .unwrap_or_default();
                crate::library_view::draw_card(d, path, bitmap, tags);
                return Some(1);
            }
            if !GetPropW(
                d.hwndItem,
                "editor-redesign\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
            .is_null()
            {
                if matches!(d.CtlID, 201 | 204 | 205 | 207 | 209 | 213 | 214 | 218) {
                    draw_icon_button(d);
                    return Some(1);
                }
                if d.CtlID == 216 {
                    let mut words = [0u16; 64];
                    let n = GetWindowTextW(d.hwndItem, words.as_mut_ptr(), 64);
                    let on = String::from_utf16_lossy(&words[..n as usize]).ends_with("ON");
                    let t = style_theme();
                    FillRect(d.hDC, &d.rcItem, style::brush(t.bg));
                    let mut label = d.rcItem;
                    label.right -= 58;
                    style::label(
                        d.hDC,
                        "Trim preview",
                        label,
                        style::font(&crate::display::font_name(), 14, 400),
                        t.muted,
                        DT_SINGLELINE | DT_VCENTER,
                    );
                    let r = RECT {
                        left: d.rcItem.right - 46,
                        top: d.rcItem.top + 6,
                        right: d.rcItem.right,
                        bottom: d.rcItem.top + 30,
                    };
                    rounded(
                        d.hDC,
                        r,
                        if on { t.accent } else { t.panel },
                        if GetFocus() == d.hwndItem {
                            t.text
                        } else {
                            t.border
                        },
                        24,
                    );
                    let x = if on { r.right - 21 } else { r.left + 3 };
                    style::disc(d.hDC, RECT { left: x, top: r.top + 3, right: x + 18, bottom: r.top + 21 }, if on { t.bg } else { t.muted }, None);
                    return Some(1);
                }
                let mut text = [0u16; 200];
                let n = GetWindowTextW(d.hwndItem, text.as_mut_ptr(), 200);
                let title = String::from_utf16_lossy(&text[..n as usize]);
                let mut pt = POINT::default();
                GetCursorPos(&mut pt);
                let mut bounds = RECT::default();
                GetWindowRect(d.hwndItem, &mut bounds);
                let disabled = d.itemState & ODS_DISABLED != 0;
                let state = style::State {
                    hover: !disabled
                        && pt.x >= bounds.left
                        && pt.x < bounds.right
                        && pt.y >= bounds.top
                        && pt.y < bounds.bottom,
                    pressed: d.itemState & ODS_SELECTED != 0,
                    focused: GetFocus() == d.hwndItem,
                    disabled,
                    ..Default::default()
                };
                let kind = if d.CtlID == 206 {
                    style::Button::Primary
                } else if d.CtlID == 214 {
                    style::Button::Danger
                } else {
                    style::Button::Default
                };
                let title = if d.CtlID == 201 {
                    if GetPropW(
                        d.hwndItem,
                        "playing\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
                    )
                    .is_null()
                    {
                        "Play"
                    } else {
                        "Pause"
                    }
                } else if d.CtlID == 207 {
                    "Open"
                } else {
                    &title
                };
                let mut origin = POINT { x: 0, y: 0 };
                ClientToScreen(d.hwndItem, &mut origin);
                ScreenToClient(hwnd, &mut origin);
                FillRect(d.hDC, &d.rcItem, style::brush(bg()));
                style::button(
                    d.hDC,
                    d.rcItem,
                    title,
                    kind,
                    state,
                    style_theme(),
                    style::font(&crate::display::font_name(), 14, 400),
                );
                if matches!(d.CtlID, 215 | 217) {
                    let x = d.rcItem.left + 17;
                    let y = (d.rcItem.top + d.rcItem.bottom) / 2;
                    if d.CtlID == 217 {
                        style::lines(d.hDC, &[(x + 3, y - 6), (x - 3, y), (x + 3, y + 6)], 2., text_color());
                    } else {
                        for (dy, knob) in [(-5, -2), (5, 3)] {
                            style::lines(d.hDC, &[(x - 6, y + dy), (x + 6, y + dy)], 2., text_color());
                            style::lines(d.hDC, &[(x + knob, y + dy - 3), (x + knob, y + dy + 3)], 2., text_color());
                        }
                    }
                }
                return Some(1);
            }
            if d.CtlID == 14
                && !GetPropW(
                    hwnd,
                    "display-settings\0"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                        .as_ptr(),
                )
                .is_null()
            {
                let t = style_theme();
                FillRect(d.hDC, &d.rcItem, style::brush(t.bg));
                style::button(
                    d.hDC,
                    d.rcItem,
                    "",
                    style::Button::Default,
                    style::State::default(),
                    t,
                    style::font(&crate::display::font_name(), 14, 600),
                );
                style::lines(d.hDC, &[(24, 14), (18, 20), (24, 26)], 2., t.text);
                let mut r = d.rcItem;
                r.left += 20;
                style::label(
                    d.hDC,
                    "Back to replay",
                    r,
                    style::font(&crate::display::font_name(), 14, 600),
                    t.text,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                return Some(1);
            }
            if (82..=84).contains(&d.CtlID) {
                let t = style_theme();
                let selected = !GetPropW(
                    d.hwndItem,
                    "section-active\0"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                        .as_ptr(),
                )
                .is_null();
                FillRect(d.hDC, &d.rcItem, style::brush(t.bg));
                if selected {
                    style::rect(d.hDC, d.rcItem, t.raise, t.raise, 12);
                }
                let labels = ["Capture", "Appearance", "Files / app"];
                let mut r = d.rcItem;
                r.left += 40;
                style::label(
                    d.hDC,
                    labels[(d.CtlID - 82) as usize],
                    r,
                    style::font(
                        &crate::display::font_name(),
                        14,
                        if selected { 600 } else { 400 },
                    ),
                    if selected { t.text } else { t.muted },
                    DT_LEFT | DT_SINGLELINE | DT_VCENTER,
                );
                let old = SelectObject(
                    d.hDC,
                    style::pen(if selected { t.accent } else { t.muted }, 1) as _,
                );
                let brush = SelectObject(d.hDC, GetStockObject(NULL_BRUSH));
                match d.CtlID {
                    82 => {
                        Rectangle(d.hDC, 13, 16, 24, 27);
                        MoveToEx(d.hDC, 24, 19, std::ptr::null_mut());
                        LineTo(d.hDC, 29, 16);
                        LineTo(d.hDC, 29, 28);
                        LineTo(d.hDC, 24, 25);
                    }
                    83 => {
                        Ellipse(d.hDC, 12, 13, 30, 31);
                        for (x, y) in [(17, 18), (23, 17), (26, 23)] {
                            Ellipse(d.hDC, x, y, x + 3, y + 3);
                        }
                    }
                    _ => {
                        MoveToEx(d.hDC, 12, 17, std::ptr::null_mut());
                        LineTo(d.hDC, 18, 17);
                        LineTo(d.hDC, 21, 20);
                        LineTo(d.hDC, 29, 20);
                        LineTo(d.hDC, 29, 29);
                        LineTo(d.hDC, 12, 29);
                        LineTo(d.hDC, 12, 17);
                    }
                }
                SelectObject(d.hDC, old);
                SelectObject(d.hDC, brush);
                if GetFocus() == d.hwndItem {
                    style::focus(d.hDC, d.rcItem, t);
                }
                return Some(1);
            }
            if crate::replay_design::draw_control(d) {
                return Some(1);
            }
            if d.CtlType == ODT_COMBOBOX {
                let closed = d.itemState & ODS_COMBOBOXEDIT != 0;
                let selected = !closed && d.itemState & ODS_SELECTED != 0;
                let fill = if selected {
                    blend(panel(), accent())
                } else {
                    panel()
                };
                let b = CreateSolidBrush(panel());
                FillRect(d.hDC, &d.rcItem, b);
                DeleteObject(b);
                if selected {
                    let mut rr = d.rcItem;
                    rr.left += 4;
                    rr.right -= 4;
                    rr.top += 1;
                    rr.bottom -= 1;
                    rounded(d.hDC, rr, fill, fill, 12);
                }
                if d.itemID != u32::MAX {
                    let mut value = [0u16; 256];
                    let len = SendMessageW(
                        d.hwndItem,
                        CB_GETLBTEXT,
                        d.itemID as usize,
                        value.as_mut_ptr() as isize,
                    )
                    .max(0) as i32;
                    let old = SelectObject(d.hDC, SendMessageW(d.hwndItem, WM_GETFONT, 0, 0) as _);
                    SetBkMode(d.hDC, TRANSPARENT as _);
                    SetTextColor(d.hDC, text_color());
                    let mut r = d.rcItem;
                    r.left += 14;
                    r.right -= 12;
                    DrawTextW(
                        d.hDC,
                        value.as_ptr(),
                        len,
                        &mut r,
                        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                    );
                    SelectObject(d.hDC, old);
                    if selected {
                        let p = CreatePen(PS_SOLID, 3, accent());
                        let old = SelectObject(d.hDC, p);
                        MoveToEx(d.hDC, r.left - 10, r.top + 8, std::ptr::null_mut());
                        LineTo(d.hDC, r.left - 10, r.bottom - 8);
                        SelectObject(d.hDC, old);
                        DeleteObject(p);
                    }
                }
                return Some(1);
            }
            if matches!(d.CtlID, 201 | 204 | 205 | 207 | 209 | 213 | 214 | 218) {
                draw_icon_button(d);
                return Some(1);
            }
            let disabled = d.itemState & ODS_DISABLED != 0;
            let down = d.itemState & ODS_SELECTED != 0;
            let mut point = POINT::default();
            GetCursorPos(&mut point);
            let mut bounds = RECT::default();
            GetWindowRect(d.hwndItem, &mut bounds);
            let hover = point.x >= bounds.left
                && point.x < bounds.right
                && point.y >= bounds.top
                && point.y < bounds.bottom;
            let main = d.CtlID == 1 || d.CtlID == 206;
            let chosen = if (82..=84).contains(&d.CtlID) || (110..=111).contains(&d.CtlID) || crate::ctl_ids::QUICK_LEN.contains(&(d.CtlID as usize)) {
                !GetPropW(
                    d.hwndItem,
                    "section-active\0"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                        .as_ptr(),
                )
                .is_null()
            } else if (100..=104).contains(&d.CtlID) {
                rows()
                    .lock()
                    .unwrap()
                    .get(&(d.hwndItem as usize))
                    .is_some_and(|p| crate::library::selected(p))
            } else {
                false
            };
            let fill = if chosen {
                blend(panel(), accent())
            } else if main && !disabled {
                if down {
                    blend(accent(), 0)
                } else {
                    accent()
                }
            } else {
                if down || hover {
                    blend(panel(), accent())
                } else {
                    blend(panel(), 0xffffff)
                }
            };
            let ground = CreateSolidBrush(if d.CtlID == 4 || d.CtlID == 5 {
                bg()
            } else {
                panel()
            });
            FillRect(d.hDC, &d.rcItem, ground);
            DeleteObject(ground);
            rounded(
                d.hDC,
                d.rcItem,
                fill,
                if main && !disabled {
                    fill
                } else if chosen || GetFocus() == d.hwndItem {
                    accent()
                } else {
                    border()
                },
                12,
            );
            if chosen && (82..=84).contains(&d.CtlID) {
                style::rect(
                    d.hDC,
                    RECT {
                        left: 2,
                        top: 8,
                        right: 5,
                        bottom: d.rcItem.bottom - 8,
                    },
                    accent(),
                    accent(),
                    2,
                );
            }

            SetBkMode(d.hDC, TRANSPARENT as i32);
            SetTextColor(
                d.hDC,
                if disabled {
                    muted()
                } else if main {
                    bg()
                } else {
                    text_color()
                },
            );
            let mut text = [0u16; 200];
            let len = GetWindowTextW(d.hwndItem, text.as_mut_ptr(), 200);
            let font = SendMessageW(d.hwndItem, WM_GETFONT, 0, 0);
            let old = SelectObject(d.hDC, font as _);
            let mut r = d.rcItem;
            if down {
                r.left += 1;
                r.top += 1;
            }
            if (100..=104).contains(&d.CtlID) {
                let large = r.bottom - r.top > 45;
                let tw = if large {
                    if r.right < 300 {
                        64
                    } else {
                        86
                    }
                } else {
                    55
                };
                let th = if large {
                    54.min(r.bottom - r.top - 12)
                } else {
                    30
                };
                let ty = (r.bottom - r.top - th) / 2;
                rounded(
                    d.hDC,
                    RECT {
                        left: 5,
                        top: ty,
                        right: 5 + tw,
                        bottom: ty + th,
                    },
                    bg(),
                    border(),
                    10,
                );
                let pen = CreatePen(PS_SOLID, 2, accent());
                let prev = SelectObject(d.hDC, pen);
                let cx = 5 + tw / 2;
                let cy = ty + th / 2;
                MoveToEx(d.hDC, cx - 4, cy - 7, std::ptr::null_mut());
                LineTo(d.hDC, cx + 7, cy);
                LineTo(d.hDC, cx - 4, cy + 7);
                LineTo(d.hDC, cx - 4, cy - 7);
                SelectObject(d.hDC, prev);
                DeleteObject(pen);
                if let Some(path) = rows().lock().unwrap().get(&(d.hwndItem as usize)) {
                    if let Some(bitmap) = images().lock().unwrap().get(path) {
                        let src = CreateCompatibleDC(d.hDC);
                        let prev = SelectObject(src, *bitmap as _);
                        let mut info: BITMAP = std::mem::zeroed();
                        GetObjectW(
                            *bitmap as _,
                            std::mem::size_of::<BITMAP>() as i32,
                            &mut info as *mut _ as _,
                        );
                        SetStretchBltMode(d.hDC, HALFTONE);
                        SaveDC(d.hDC);
                        let mask = CreateRoundRectRgn(5, ty, 5 + tw, ty + th, 10, 10);
                        SelectClipRgn(d.hDC, mask);
                        StretchBlt(
                            d.hDC,
                            5,
                            ty,
                            tw,
                            th,
                            src,
                            0,
                            0,
                            info.bmWidth,
                            info.bmHeight,
                            SRCCOPY,
                        );
                        RestoreDC(d.hDC, -1);
                        DeleteObject(mask);
                        SelectObject(src, prev);
                        DeleteDC(src);
                    }
                }
                if chosen {
                    let pen = CreatePen(PS_SOLID, 2, accent());
                    let prev = SelectObject(d.hDC, pen);
                    let x = 5 + tw - 12;
                    let y = ty + 12;
                    rounded(
                        d.hDC,
                        RECT {
                            left: x - 8,
                            top: y - 8,
                            right: x + 8,
                            bottom: y + 8,
                        },
                        bg(),
                        accent(),
                        6,
                    );
                    MoveToEx(d.hDC, x - 4, y, std::ptr::null_mut());
                    LineTo(d.hDC, x - 1, y + 3);
                    LineTo(d.hDC, x + 5, y - 4);
                    SelectObject(d.hDC, prev);
                    DeleteObject(pen);
                }
                r.left += tw + 14;
                r.right -= 8;
                if large {
                    r.top += if r.bottom - r.top < 64 { 4 } else { 10 };
                    let split = text.iter().position(|c| *c == 10).unwrap_or(len as usize);
                    let mut first = r;
                    first.bottom = first.top + 21;
                    DrawTextW(
                        d.hDC,
                        text.as_ptr(),
                        split as i32,
                        &mut first,
                        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                    );
                    if split < len as usize {
                        r.top += 23;
                        r.right = (r.left + 68).min(r.right);
                        DrawTextW(
                            d.hDC,
                            text.as_ptr().add(split + 1),
                            len - split as i32 - 1,
                            &mut r,
                            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                        );
                    }
                } else {
                    for ch in &mut text {
                        if *ch == 10 {
                            *ch = 32;
                        }
                    }
                    DrawTextW(
                        d.hDC,
                        text.as_ptr(),
                        len,
                        &mut r,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                    );
                }
                if large {
                    let data = badges()
                        .lock()
                        .unwrap()
                        .get(&(d.hwndItem as usize))
                        .cloned()
                        .unwrap_or_default();
                    let mut x = tw + 86;
                    let y = d.rcItem.top
                        + if d.rcItem.bottom - d.rcItem.top < 64 {
                            25
                        } else {
                            31
                        };
                    let end = d.rcItem.right - 8;
                    for (i, (tag, color)) in data.iter().enumerate() {
                        let wide: Vec<u16> = tag.encode_utf16().collect();
                        let mut size = SIZE::default();
                        GetTextExtentPoint32W(d.hDC, wide.as_ptr(), wide.len() as i32, &mut size);
                        let width = size.cx + 14;
                        let remaining = data.len() - i;
                        let (label, width) = if x + width > end {
                            (format!("+{remaining}"), 30)
                        } else {
                            (tag.clone(), width)
                        };
                        if x + width > end {
                            break;
                        }
                        let rr = RECT {
                            left: x,
                            top: y,
                            right: x + width,
                            bottom: y + 19,
                        };
                        rounded(d.hDC, rr, blend(fill, *color), blend(fill, *color), 10);
                        SetTextColor(d.hDC, *color);
                        let wide: Vec<u16> = label.encode_utf16().collect();
                        let mut tr = RECT { left: x + 7, ..rr };
                        DrawTextW(
                            d.hDC,
                            wide.as_ptr(),
                            wide.len() as i32,
                            &mut tr,
                            DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
                        );
                        x += width + 5;
                        if label.starts_with('+') {
                            break;
                        }
                    }
                }
                SelectObject(d.hDC, old);
                return Some(1);
            }
            if d.CtlID < 100
                && d.CtlID != 11
                && d.CtlID != 36
                && d.CtlID != 40
                && d.CtlID != 41
                && d.CtlID != 43
                && d.CtlID != 44
                && d.CtlID != 70
                && d.CtlID != 71
                && d.CtlID != 72
            {
                let pen = CreatePen(PS_SOLID, 2, if main && !disabled { bg() } else { accent() });
                let oldpen = SelectObject(d.hDC, pen);
                let x = r.left + 18;
                let y = (r.top + r.bottom) / 2;
                match d.CtlID {
                    1 => {
                        MoveToEx(d.hDC, x, y - 7, std::ptr::null_mut());
                        LineTo(d.hDC, x, y + 6);
                        MoveToEx(d.hDC, x - 5, y + 1, std::ptr::null_mut());
                        LineTo(d.hDC, x, y + 6);
                        LineTo(d.hDC, x + 5, y + 1);
                    }
                    10 => {
                        MoveToEx(d.hDC, x - 3, y - 6, std::ptr::null_mut());
                        LineTo(d.hDC, x - 3, y + 6);
                        MoveToEx(d.hDC, x + 3, y - 6, std::ptr::null_mut());
                        LineTo(d.hDC, x + 3, y + 6);
                    }
                    2 => {
                        let b = SelectObject(d.hDC, GetStockObject(NULL_BRUSH));
                        Rectangle(d.hDC, x - 7, y - 4, x + 7, y + 6);
                        MoveToEx(d.hDC, x - 7, y - 4, std::ptr::null_mut());
                        LineTo(d.hDC, x - 7, y - 7);
                        LineTo(d.hDC, x - 1, y - 7);
                        LineTo(d.hDC, x + 2, y - 4);
                        SelectObject(d.hDC, b);
                    }
                    7 => {
                        MoveToEx(d.hDC, x - 6, y - 3, std::ptr::null_mut());
                        LineTo(d.hDC, x - 2, y - 3);
                        LineTo(d.hDC, x + 3, y - 7);
                        LineTo(d.hDC, x + 3, y + 7);
                        LineTo(d.hDC, x - 2, y + 3);
                        LineTo(d.hDC, x - 6, y + 3);
                        LineTo(d.hDC, x - 6, y - 3);
                    }
                    14 => {
                        let b = SelectObject(d.hDC, GetStockObject(NULL_BRUSH));
                        Ellipse(d.hDC, x - 6, y - 6, x + 6, y + 6);
                        Ellipse(d.hDC, x - 2, y - 2, x + 2, y + 2);
                        for (dx, dy) in [(0, -9), (0, 9), (-9, 0), (9, 0)] {
                            MoveToEx(d.hDC, x + dx * 2 / 3, y + dy * 2 / 3, std::ptr::null_mut());
                            LineTo(d.hDC, x + dx, y + dy);
                        }
                        SelectObject(d.hDC, b);
                    }
                    12 => {
                        let b = SelectObject(d.hDC, GetStockObject(NULL_BRUSH));
                        RoundRect(d.hDC, x - 3, y - 8, x + 3, y + 2, 5, 5);
                        SelectObject(d.hDC, b);
                        MoveToEx(d.hDC, x - 6, y - 2, std::ptr::null_mut());
                        LineTo(d.hDC, x - 6, y + 4);
                        LineTo(d.hDC, x + 6, y + 4);
                        LineTo(d.hDC, x + 6, y - 2);
                        MoveToEx(d.hDC, x, y + 4, std::ptr::null_mut());
                        LineTo(d.hDC, x, y + 8);
                    }
                    6 => {
                        MoveToEx(d.hDC, x - 6, y + 4, std::ptr::null_mut());
                        LineTo(d.hDC, x - 6, y);
                        MoveToEx(d.hDC, x, y + 4, std::ptr::null_mut());
                        LineTo(d.hDC, x, y - 3);
                        MoveToEx(d.hDC, x + 6, y + 4, std::ptr::null_mut());
                        LineTo(d.hDC, x + 6, y - 7);
                    }
                    4 | 81 => {
                        let pts = [
                            (-5, -6),
                            (-8, -2),
                            (-8, 3),
                            (-5, 7),
                            (1, 8),
                            (6, 5),
                            (8, 1),
                            (7, -4),
                            (3, -7),
                        ];
                        MoveToEx(d.hDC, x + pts[0].0, y + pts[0].1, std::ptr::null_mut());
                        for (dx, dy) in &pts[1..] {
                            LineTo(d.hDC, x + dx, y + dy);
                        }
                        MoveToEx(d.hDC, x + 3, y - 10, std::ptr::null_mut());
                        LineTo(d.hDC, x + 3, y - 4);
                        LineTo(d.hDC, x + 9, y - 4);
                    }
                    5 => {
                        MoveToEx(d.hDC, x - 7, y + 6, std::ptr::null_mut());
                        LineTo(d.hDC, x - 7, y);
                        LineTo(d.hDC, x - 3, y);
                        LineTo(d.hDC, x, y - 6);
                        LineTo(d.hDC, x + 3, y + 4);
                        LineTo(d.hDC, x + 7, y + 4);
                    }
                    34 | 35 => {
                        MoveToEx(d.hDC, x - 7, y + 4, std::ptr::null_mut());
                        for (dx, dy) in [
                            (-5, 1),
                            (-5, -4),
                            (-2, -7),
                            (2, -7),
                            (5, -4),
                            (5, 1),
                            (7, 4),
                            (-7, 4),
                        ] {
                            LineTo(d.hDC, x + dx, y + dy);
                        }
                        MoveToEx(d.hDC, x - 2, y + 8, std::ptr::null_mut());
                        LineTo(d.hDC, x + 2, y + 8);
                    }
                    _ => {}
                }
                SelectObject(d.hDC, oldpen);
                DeleteObject(pen);
                if [1, 2, 4, 5, 6, 7, 10, 12, 14, 34, 35, 81].contains(&d.CtlID) {
                    r.left += 20;
                }
            }
            r.left += if d.CtlID == 85 { 6 } else { 12 };
            r.right -= if d.CtlID == 85 { 6 } else { 12 };
            DrawTextW(
                d.hDC,
                text.as_ptr(),
                len,
                &mut r,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            SelectObject(d.hDC, old);
            Some(1)
        }
        _ => None,
    }
}
#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: HWND, attr: u32, value: *const i32, size: u32) -> i32;
}
pub unsafe fn titlebar(hwnd: HWND) {
    let enabled = 1;
    DwmSetWindowAttribute(hwnd, 20, &enabled, 4);
}

fn wide_hover() -> Vec<u16> {
    "hover-state".encode_utf16().chain(Some(0)).collect()
}
unsafe extern "system" fn hover_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _id: usize,
    _data: usize,
) -> isize {
    if m == WM_TIMER && w == motion::TIMER {
        motion::tick(h);
        return 0;
    }
    if (m == WM_ENABLE || m == WM_SHOWWINDOW) && w == 0 {
        motion::forget(h);
    }
    if m == WM_NCDESTROY {
        motion::forget(h);
    }
    if m == WM_ERASEBKGND {
        return 1;
    }
    if m == WM_MOUSEWHEEL {
        return SendMessageW(GetParent(h), m, w, l);
    }
    if m == WM_MOUSEMOVE {
        let mut t = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as _,
            dwFlags: TME_LEAVE,
            hwndTrack: h,
            dwHoverTime: 0,
        };
        windows_sys::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut t);
        if GetPropW(h, wide_hover().as_ptr()).is_null() {
            SetPropW(h, wide_hover().as_ptr(), 1usize as _);
            InvalidateRect(h, std::ptr::null(), 0);
        }
    }
    if m == WM_CONTEXTMENU && (100..=104).contains(&GetDlgCtrlID(h)) {
        let row = GetDlgCtrlID(h) - 100;
        PostThreadMessageW(
            windows_sys::Win32::System::Threading::GetCurrentThreadId(),
            WM_HOTKEY,
            200 + row as usize,
            0,
        );
        return 0;
    }
    if m == WM_MOUSELEAVE {
        RemovePropW(h, wide_hover().as_ptr());
        InvalidateRect(h, std::ptr::null(), 0);
    }
    if m == WM_NCDESTROY {
        RemoveWindowSubclass(h, Some(hover_proc), 1);
    }
    DefSubclassProc(h, m, w, l)
}
unsafe extern "system" fn card_proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            let d = DRAWITEMSTRUCT {
                CtlType: ODT_BUTTON,
                CtlID: GetDlgCtrlID(h) as u32,
                itemID: 0,
                itemAction: ODA_DRAWENTIRE,
                itemState: if GetCapture() == h { ODS_SELECTED } else { 0 }
                    | if GetFocus() == h { ODS_FOCUS } else { 0 },
                hwndItem: h,
                hDC: dc,
                rcItem: r,
                itemData: 0,
            };
            SendMessageW(
                GetParent(h),
                WM_DRAWITEM,
                GetDlgCtrlID(h) as usize,
                &d as *const _ as isize,
            );
            EndPaint(h, &ps);
            0
        }
        WM_LBUTTONDOWN => {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(h);
            SetCapture(h);
            InvalidateRect(h, std::ptr::null(), 0);
            0
        }
        WM_LBUTTONUP => {
            let pressed = GetCapture() == h;
            ReleaseCapture();
            InvalidateRect(h, std::ptr::null(), 0);
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            let x = l as i16 as i32;
            let y = (l >> 16) as i16 as i32;
            if pressed && x >= 0 && y >= 0 && x < r.right && y < r.bottom {
                // The trash icon on a library card asks to delete that clip (confirmed in the app).
                let id = GetDlgCtrlID(h);
                if (100..=104).contains(&id) && !crate::library::state().lock().unwrap().active {
                    let st = crate::library_view::star_hit(h);
                    if x >= st.left && x < st.right && y >= st.top && y < st.bottom {
                        PostThreadMessageW(
                            windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                            WM_HOTKEY,
                            220 + (id - 100) as usize,
                            0,
                        );
                        return 0;
                    }
                    let t = crate::library_view::trash_hit(h);
                    if x >= t.left && x < t.right && y >= t.top && y < t.bottom {
                        PostThreadMessageW(
                            windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                            WM_HOTKEY,
                            210 + (id - 100) as usize,
                            0,
                        );
                        return 0;
                    }
                }
                SendMessageW(
                    GetParent(h),
                    WM_COMMAND,
                    GetDlgCtrlID(h) as usize,
                    h as isize,
                );
            }
            0
        }
        WM_KEYDOWN if w == 32 || w == 13 => {
            SendMessageW(
                GetParent(h),
                WM_COMMAND,
                GetDlgCtrlID(h) as usize,
                h as isize,
            );
            0
        }
        WM_SETFONT => {
            SetPropW(
                h,
                "card-font\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
                w as _,
            );
            if l != 0 {
                InvalidateRect(h, std::ptr::null(), 0);
            }
            0
        }
        WM_GETFONT => {
            GetPropW(h, "card-font\0".encode_utf16().collect::<Vec<_>>().as_ptr()) as isize
        }
        WM_SETTEXT => {
            let v = DefWindowProcW(h, m, w, l);
            InvalidateRect(h, std::ptr::null(), 0);
            v
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            InvalidateRect(h, std::ptr::null(), 0);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
pub unsafe fn replay_card(
    parent: HWND,
    id: usize,
    font: windows_sys::Win32::Graphics::Gdi::HFONT,
) -> HWND {
    let cls: Vec<u16> = "ZimClipReplayCard\0".encode_utf16().collect();
    let inst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(card_proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let h = CreateWindowExW(
        0,
        cls.as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        630,
        180 + (id - 100) as i32 * 58,
        274,
        56,
        parent,
        id as _,
        inst,
        std::ptr::null(),
    );
    SendMessageW(h, WM_SETFONT, font as usize, 0);
    interactive(h);
    h
}
pub unsafe fn interactive(h: HWND) {
    SetWindowSubclass(h, Some(hover_proc), 1, 0);
}

fn wide_prop(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe extern "system" fn search_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _id: usize,
    _data: usize,
) -> isize {
    if m == WM_SIZE {
        let result = DefSubclassProc(h, m, w, l);
        // Rounded corners come from the window region; resizing resets it.
        clip_round(h, SEARCH_RADIUS);
        InvalidateRect(h, std::ptr::null(), 0);
        return result;
    }
    let result = DefSubclassProc(h, m, w, l);
    if matches!(m, WM_SETFOCUS | WM_KILLFOCUS) {
        RedrawWindow(
            h,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_FRAME | RDW_INVALIDATE,
        );
    }
    if m == WM_PAINT
        && GetWindowTextLengthW(h) == 0
        && GetFocus() != h
        && GetPropW(h, wide_prop("no-cue").as_ptr()).is_null()
    {
        let dc = GetDC(h);
        let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
        SetBkMode(dc, TRANSPARENT as _);
        SetTextColor(dc, muted());
        let mut r = RECT::default();
        GetClientRect(h, &mut r);
        r.left += SEARCH_INSET;
        let text: Vec<u16> = "Search clips or tags...".encode_utf16().collect();
        DrawTextW(
            dc,
            text.as_ptr(),
            text.len() as i32,
            &mut r,
            DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, old);
        ReleaseDC(h, dc);
    }
    if m == WM_PAINT {
        // Native edit controls only have square borders, so draw a rounded outline
        // inside the rounded window region.
        let dc = GetDC(h);
        let mut r = RECT::default();
        GetClientRect(h, &mut r);
        let pen = CreatePen(
            PS_SOLID,
            1,
            if GetFocus() == h { accent() } else { border() },
        );
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        RoundRect(dc, 0, 0, r.right, r.bottom, SEARCH_RADIUS, SEARCH_RADIUS);
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(pen);
        ReleaseDC(h, dc);
    }
    if m == WM_NCDESTROY {
        RemoveWindowSubclass(h, Some(search_proc), 8);
    }
    result
}
/// Corner diameter for single-line text fields (matches the rounded combo boxes).
const SEARCH_RADIUS: i32 = 16;
/// Left/right text padding inside a rounded text field.
const SEARCH_INSET: i32 = 14;
pub unsafe fn search_field(h: HWND) {
    // No native border: the outline is painted rounded by `search_proc`.
    SetWindowLongPtrW(
        h,
        GWL_EXSTYLE,
        GetWindowLongPtrW(h, GWL_EXSTYLE) & !(WS_EX_CLIENTEDGE as isize),
    );
    SetWindowLongPtrW(
        h,
        GWL_STYLE,
        GetWindowLongPtrW(h, GWL_STYLE) & !(WS_BORDER as isize),
    );
    SendMessageW(
        h,
        EM_SETMARGINS,
        (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
        ((SEARCH_INSET as isize) | ((SEARCH_INSET as isize) << 16)) as isize,
    );
    SetWindowPos(
        h,
        std::ptr::null_mut(),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
    );
    SetWindowSubclass(h, Some(search_proc), 8, 0);
    clip_round(h, SEARCH_RADIUS);
}
unsafe extern "system" fn combo_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _id: usize,
    _data: usize,
) -> isize {
    match m {
        WM_SIZE => {
            let result = DefSubclassProc(h, m, w, l);
            // Resize invalidates the region established at creation. A stale
            // region makes the visible custom arrow ignore mouse input.
            SetWindowRgn(h, std::ptr::null_mut(), 1);
            result
        }

        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let (dc, buffer) = begin_buffer(h, &mut ps);
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(h, &mut r);
            let brush = CreateSolidBrush(panel());
            FillRect(dc, &r, brush);
            DeleteObject(brush);
            rounded(
                dc,
                r,
                panel(),
                if GetFocus() == h { accent() } else { border() },
                16,
            );
            let mut value = [0u16; 256];
            let index = SendMessageW(h, CB_GETCURSEL, 0, 0);
            let len = if index >= 0 {
                SendMessageW(h, CB_GETLBTEXT, index as usize, value.as_mut_ptr() as isize).max(0)
                    as i32
            } else {
                0
            };
            let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
            SetBkMode(dc, TRANSPARENT as _);
            SetTextColor(
                dc,
                if IsWindowEnabled(h) != 0 {
                    text_color()
                } else {
                    muted()
                },
            );
            r.left += 12;
            r.right -= 34;
            DrawTextW(
                dc,
                value.as_ptr(),
                len,
                &mut r,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );
            SelectObject(dc, old);
            let pen = CreatePen(
                PS_SOLID,
                2,
                if IsWindowEnabled(h) != 0 {
                    accent()
                } else {
                    muted()
                },
            );
            let old = SelectObject(dc, pen);
            let x = r.right + 16;
            let y = (r.top + r.bottom) / 2;
            MoveToEx(dc, x - 4, y - 2, std::ptr::null_mut());
            LineTo(dc, x, y + 2);
            LineTo(dc, x + 4, y - 2);
            SelectObject(dc, old);
            DeleteObject(pen);
            end_buffer(h, &ps, dc, buffer);
            0
        }
        WM_ENABLE | WM_SETFOCUS | WM_KILLFOCUS => {
            let v = DefSubclassProc(h, m, w, l);
            InvalidateRect(h, std::ptr::null(), 0);
            v
        }
        WM_LBUTTONDOWN => {
            // The custom chevron occupies a wider area than the hidden native
            // arrow. Own the full closed surface rather than native hit tests.
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(h);
            let open = SendMessageW(h, CB_GETDROPPEDSTATE, 0, 0) == 0;
            SendMessageW(h, CB_SHOWDROPDOWN, open as usize, 0);
            InvalidateRect(h, std::ptr::null(), 0);
            0
        }
        WM_LBUTTONUP => 0,
        CB_SHOWDROPDOWN => {
            let result = DefSubclassProc(h, m, w, l);
            if w != 0 {
                animate_list(h);
            }
            InvalidateRect(h, std::ptr::null(), 0);
            result
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(h, Some(combo_proc), 2);
            DefSubclassProc(h, m, w, l)
        }
        _ => DefSubclassProc(h, m, w, l),
    }
}
pub unsafe fn dropdown(h: HWND) {
    let count = SendMessageW(h, CB_GETCOUNT, 0, 0).clamp(1, 12) as i32;
    let mut r = RECT::default();
    GetWindowRect(h, &mut r);
    SetWindowPos(
        h,
        std::ptr::null_mut(),
        0,
        0,
        r.right - r.left,
        38 + count * 34 + 12,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    SendMessageW(h, CB_SETMINVISIBLE, count as usize, 0);
    SendMessageW(h, CB_SETITEMHEIGHT, usize::MAX, 32);
    SendMessageW(h, CB_SETITEMHEIGHT, 0, 34);
    SetWindowTheme(h, [0u16].as_ptr(), [0u16].as_ptr());
    SetWindowSubclass(h, Some(combo_proc), 2, 0);
    let mut info: COMBOBOXINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of_val(&info) as _;
    if GetComboBoxInfo(h, &mut info) != 0 && !info.hwndList.is_null() {
        SetWindowTheme(info.hwndList, [0u16].as_ptr(), [0u16].as_ptr());
        SetWindowSubclass(info.hwndList, Some(list_proc), 10, 0);
    }
    clip_round(h, 16);
}

// Native combo popups must keep their normal show/paint lifecycle. Hiding the
// popup and re-showing it with AnimateWindow can expose a white native surface.
unsafe fn animate_list(h: HWND) {
    let mut info: COMBOBOXINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of_val(&info) as _;
    if GetComboBoxInfo(h, &mut info) != 0 && !info.hwndList.is_null() {
        SetWindowTheme(info.hwndList, [0u16].as_ptr(), [0u16].as_ptr());
        SetWindowSubclass(info.hwndList, Some(list_proc), 10, 0);
        clip_round(info.hwndList, 16);
        RedrawWindow(
            info.hwndList,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE | RDW_FRAME,
        );
    }
}

pub unsafe fn clip_round(h: HWND, radius: i32) {
    let mut r: RECT = std::mem::zeroed();
    GetWindowRect(h, &mut r);
    let region = CreateRoundRectRgn(
        0,
        0,
        r.right - r.left + 1,
        r.bottom - r.top + 1,
        radius,
        radius,
    );
    if SetWindowRgn(h, region, 1) == 0 {
        DeleteObject(region);
    }
}
unsafe fn draw_icon_button(d: &DRAWITEMSTRUCT) {
    let disabled = d.itemState & ODS_DISABLED != 0;
    let down = d.itemState & ODS_SELECTED != 0;
    let mut cursor = POINT::default();
    GetCursorPos(&mut cursor);
    let mut bound = RECT::default();
    GetWindowRect(d.hwndItem, &mut bound);
    let hover = cursor.x >= bound.left
        && cursor.x < bound.right
        && cursor.y >= bound.top
        && cursor.y < bound.bottom;
    let editor = !GetPropW(
        d.hwndItem,
        "editor-redesign\0"
            .encode_utf16()
            .collect::<Vec<_>>()
            .as_ptr(),
    )
    .is_null();
    let b = CreateSolidBrush(if editor { bg() } else { panel() });
    FillRect(d.hDC, &d.rcItem, b);
    DeleteObject(b);
    rounded(
        d.hDC,
        d.rcItem,
        if hover || down {
            blend(panel(), accent())
        } else {
            blend(panel(), 0xffffff)
        },
        if GetFocus() == d.hwndItem {
            accent()
        } else {
            border()
        },
        18,
    );
    let x = (d.rcItem.left + d.rcItem.right) / 2;
    let y = (d.rcItem.top + d.rcItem.bottom) / 2;
    // All glyphs go through the anti-aliased polyline path (same as the Settings icons),
    // so diagonals and curves have no GDI stair-steps.
    let col = if disabled { muted() } else { accent() };
    let ln = |pts: &[(i32, i32)]| style::lines(d.hDC, pts, 2.2, col);
    match d.CtlID {
        201 => {
            if GetPropW(
                d.hwndItem,
                [112u16, 108, 97, 121, 105, 110, 103, 0].as_ptr(),
            ) as usize
                != 0
            {
                for dx in [-4, 4] {
                    ln(&[(x + dx, y - 8), (x + dx, y + 8)]);
                }
            } else {
                ln(&[(x - 5, y - 8), (x + 8, y), (x - 5, y + 8), (x - 5, y - 8)]);
            }
        }
        204 | 205 => {
            let sign = if d.CtlID == 204 { -1 } else { 1 };
            ln(&[(x + sign * 7, y - 8), (x + sign * 7, y + 8)]);
            ln(&[(x - sign * 5, y - 6), (x + sign * 2, y), (x - sign * 5, y + 6)]);
        }
        207 => {
            // Open: a box with an arrow leaving the top-right corner.
            ln(&[(x - 3, y - 6), (x - 8, y - 6), (x - 8, y + 8), (x + 5, y + 8), (x + 5, y + 3)]);
            ln(&[(x - 1, y + 1), (x + 9, y - 9)]);
            ln(&[(x + 2, y - 9), (x + 9, y - 9), (x + 9, y - 2)]);
        }
        209 => {
            // Restart: a 300 degree arc with an arrowhead.
            let arc: Vec<(i32, i32)> = (0..=30)
                .map(|i| {
                    let a = (-80.0 + i as f64 * 10.0).to_radians();
                    (x + (a.cos() * 9.0).round() as i32, y + (a.sin() * 9.0).round() as i32)
                })
                .collect();
            ln(&arc);
            ln(&[(x - 12, y - 5), (x - 7, y - 6), (x - 7, y - 12)]);
        }
        218 => {
            // Layout: a panel split into a wide and a narrow column.
            ln(&[(x - 9, y - 7), (x + 9, y - 7), (x + 9, y + 7), (x - 9, y + 7), (x - 9, y - 7)]);
            ln(&[(x + 2, y - 7), (x + 2, y + 7)]);
        }
        213 => {
            // Rename: pencil outline plus the tip stroke.
            ln(&[(x - 7, y + 7), (x - 5, y), (x + 5, y - 10), (x + 10, y - 5), (x, y + 5), (x - 7, y + 7)]);
            ln(&[(x + 3, y - 8), (x + 8, y - 3)]);
        }
        214 => {
            // Delete: bin body, lid, handle and two slats.
            ln(&[(x - 6, y - 5), (x - 5, y + 9), (x + 6, y + 9), (x + 7, y - 5)]);
            ln(&[(x - 9, y - 6), (x + 9, y - 6)]);
            ln(&[(x - 3, y - 9), (x + 3, y - 9)]);
            for dx in [-2, 3] {
                ln(&[(x + dx, y - 2), (x + dx, y + 6)]);
            }
        }
        _ => {
            ln(&[(x - 6, y - 6), (x + 6, y + 6)]);
            ln(&[(x + 6, y - 6), (x - 6, y + 6)]);
        }
    }
}
pub unsafe fn tooltip(control: HWND, label: &str) {
    let tip = CreateWindowExW(
        WS_EX_TOPMOST,
        [
            116u16, 111, 111, 108, 116, 105, 112, 115, 95, 99, 108, 97, 115, 115, 51, 50, 0,
        ]
        .as_ptr(),
        std::ptr::null(),
        WS_POPUP | TTS_ALWAYSTIP,
        0,
        0,
        0,
        0,
        GetParent(control),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null(),
    );
    let words: Box<[u16]> = label.encode_utf16().chain(Some(0)).collect();
    let raw = Box::into_raw(words);
    let text = (*raw).as_mut_ptr();
    let mut tool: TTTOOLINFOW = std::mem::zeroed();
    tool.cbSize = std::mem::size_of_val(&tool) as _;
    tool.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
    tool.hwnd = GetParent(control);
    tool.uId = control as usize;
    tool.lpszText = text;
    SendMessageW(tip, TTM_ADDTOOLW, 0, &tool as *const _ as isize);
    SetWindowSubclass(tip, Some(tooltip_proc), 55, raw as *mut u16 as usize);
    SetPropW(
        tip,
        [108u16, 101, 110, 0].as_ptr(),
        (label.encode_utf16().count() + 1) as _,
    );
}
unsafe extern "system" fn tooltip_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _: usize,
    data: usize,
) -> isize {
    if m == WM_NCDESTROY {
        let len = GetPropW(h, [108u16, 101, 110, 0].as_ptr()) as usize;
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            data as *mut u16,
            len,
        )));
        RemoveWindowSubclass(h, Some(tooltip_proc), 55);
    }
    DefSubclassProc(h, m, w, l)
}
pub unsafe fn modern_slider(h: HWND) {
    SetWindowSubclass(h, Some(slider_proc), 8, 0);
}
unsafe extern "system" fn slider_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _: usize,
    _: usize,
) -> isize {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
    match m {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let (dc, buffer) = begin_buffer(h, &mut ps);
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(h, &mut r);
            let b = CreateSolidBrush(
                if !GetPropW(
                    h,
                    "surface-bg\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
                )
                .is_null()
                {
                    bg()
                } else {
                    panel()
                },
            );
            FillRect(dc, &r, b);
            DeleteObject(b);
            let y = (r.top + r.bottom) / 2;
            let min = SendMessageW(h, TBM_GETRANGEMIN, 0, 0);
            let max = SendMessageW(h, TBM_GETRANGEMAX, 0, 0);
            let pos = SendMessageW(h, 0x400, 0, 0);
            let x = 12 + ((r.right - 24) as isize * (pos - min) / (max - min).max(1)) as i32;
            rounded(
                dc,
                RECT {
                    left: 12,
                    top: y - 3,
                    right: r.right - 12,
                    bottom: y + 3,
                },
                border(),
                border(),
                6,
            );
            if x > 12 {
                rounded(
                    dc,
                    RECT {
                        left: 12,
                        top: y - 3,
                        right: x,
                        bottom: y + 3,
                    },
                    accent(),
                    accent(),
                    6,
                );
            }
            if GetFocus() == h {
                rounded(
                    dc,
                    RECT {
                        left: x - 10,
                        top: y - 10,
                        right: x + 11,
                        bottom: y + 11,
                    },
                    blend(panel(), accent()),
                    border(),
                    22,
                );
            }
            rounded(
                dc,
                RECT {
                    left: x - 7,
                    top: y - 7,
                    right: x + 8,
                    bottom: y + 8,
                },
                accent(),
                accent(),
                16,
            );
            end_buffer(h, &ps, dc, buffer);
            0
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE if m == WM_LBUTTONDOWN || GetCapture() == h => {
            if m == WM_LBUTTONDOWN {
                SetCapture(h);
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(h);
            }
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            let min = SendMessageW(h, TBM_GETRANGEMIN, 0, 0);
            let max = SendMessageW(h, TBM_GETRANGEMAX, 0, 0);
            let x = (l as i16 as i32 - 12).clamp(0, (r.right - 24).max(1));
            let pos = min
                + (x as isize * (max - min) + (r.right - 24) as isize / 2)
                    / (r.right - 24).max(1) as isize;
            SendMessageW(h, TBM_SETPOS, 1, pos);
            SendMessageW(GetParent(h), WM_HSCROLL, TB_THUMBTRACK as usize, h as isize);
            InvalidateRect(h, std::ptr::null(), 0);
            0
        }
        WM_LBUTTONUP => {
            if GetCapture() == h {
                ReleaseCapture();
                SendMessageW(GetParent(h), WM_HSCROLL, TB_ENDTRACK as usize, h as isize);
            }
            0
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(h, Some(slider_proc), 8);
            DefSubclassProc(h, m, w, l)
        }
        _ => {
            let v = DefSubclassProc(h, m, w, l);
            if matches!(m, WM_KEYDOWN | WM_SETFOCUS | WM_KILLFOCUS | TBM_SETPOS) {
                InvalidateRect(h, std::ptr::null(), 0);
            }
            v
        }
    }
}

pub unsafe fn modern_hotkey(h: HWND) {
    SetWindowLongPtrW(
        h,
        GWL_EXSTYLE,
        GetWindowLongPtrW(h, GWL_EXSTYLE) & !(WS_EX_CLIENTEDGE as isize),
    );
    SetWindowLongPtrW(
        h,
        GWL_STYLE,
        GetWindowLongPtrW(h, GWL_STYLE) & !(WS_BORDER as isize),
    );
    SetWindowPos(
        h,
        std::ptr::null_mut(),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
    );
    SetWindowSubclass(h, Some(hotkey_proc), 9, 0);
    clip_round(h, 14);
}
unsafe extern "system" fn hotkey_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _: usize,
    _: usize,
) -> isize {
    match m {
        WM_SIZE => {
            clip_round(h, 14);
            InvalidateRect(h, std::ptr::null(), 0);
            0
        }
        WM_GETDLGCODE => DLGC_WANTALLKEYS as isize | DLGC_WANTARROWS as isize,
        WM_SETFOCUS | WM_KILLFOCUS => {
            PostThreadMessageW(
                windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                WM_HOTKEY,
                if m == WM_SETFOCUS { 47 } else { 48 },
                0,
            );
            let v = DefSubclassProc(h, m, w, l);
            InvalidateRect(h, std::ptr::null(), 0);
            v
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
            if w == 0x09 {
                let mut next = GetDlgItem(GetParent(h), if GetDlgCtrlID(h) == crate::ctl_ids::PAUSE_KEY as i32 { crate::ctl_ids::APPLY_PAUSE as i32 } else { 32 });
                if next.is_null() {
                    // First-run setup has no id 32; move to its Next button instead.
                    next = GetDlgItem(GetParent(h), 1001);
                }
                if !next.is_null() {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(next);
                }
                return 0;
            }
            if ![0x10, 0x11, 0x12, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5].contains(&w) {
                let flags = if GetKeyState(0x10) < 0 { 1 } else { 0 }
                    | if GetKeyState(0x11) < 0 { 2 } else { 0 }
                    | if GetKeyState(0x12) < 0 { 4 } else { 0 };
                SendMessageW(h, 0x401, w | (flags << 8), 0);
            }
            InvalidateRect(h, std::ptr::null(), 0);
            0
        }
        WM_KEYUP | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR => 0,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let (dc, buffer) = begin_buffer(h, &mut ps);
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            let b = CreateSolidBrush(panel());
            FillRect(dc, &r, b);
            DeleteObject(b);
            rounded(
                dc,
                r,
                panel(),
                if GetFocus() == h { accent() } else { border() },
                14,
            );
            let key = SendMessageW(h, HKM_GETHOTKEY, 0, 0) as u32;
            let mut parts = Vec::new();
            if key & 0x200 != 0 {
                parts.push("Ctrl".to_string());
            }
            if key & 0x400 != 0 {
                parts.push("Alt".to_string());
            }
            if key & 0x100 != 0 {
                parts.push("Shift".to_string());
            }
            let mut name = [0u16; 64];
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                GetKeyNameTextW, MapVirtualKeyW,
            };
            let sc = MapVirtualKeyW(key & 255, 0);
            let n = GetKeyNameTextW(
                ((sc << 16) | if key & 0x800 != 0 { 1 << 24 } else { 0 }) as i32,
                name.as_mut_ptr(),
                64,
            );
            if key & 255 != 0 && n > 0 {
                parts.push(String::from_utf16_lossy(&name[..n as usize]));
            }
            if parts.is_empty() {
                parts.push("Press a shortcut".into());
            }
            let value: Vec<u16> = parts.join(" + ").encode_utf16().collect();
            let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
            SetBkMode(dc, TRANSPARENT as _);
            SetTextColor(dc, text_color());
            r.left += 12;
            r.right -= 10;
            DrawTextW(
                dc,
                value.as_ptr(),
                value.len() as i32,
                &mut r,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );
            SelectObject(dc, old);
            end_buffer(h, &ps, dc, buffer);
            0
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(h, Some(hotkey_proc), 9);
            DefSubclassProc(h, m, w, l)
        }
        _ => {
            let v = DefSubclassProc(h, m, w, l);
            if matches!(
                m,
                WM_KEYDOWN | WM_KEYUP | WM_SETFOCUS | WM_KILLFOCUS | HKM_SETHOTKEY
            ) {
                InvalidateRect(h, std::ptr::null(), 0);
            }
            v
        }
    }
}

unsafe extern "system" fn list_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _: usize,
    _: usize,
) -> isize {
    if m == WM_NCDESTROY {
        RemoveWindowSubclass(h, Some(list_proc), 10);
    }
    if m == WM_ERASEBKGND {
        let mut r = RECT::default();
        GetClientRect(h, &mut r);
        let b = CreateSolidBrush(panel());
        FillRect(w as _, &r, b);
        DeleteObject(b);
        return 1;
    }
    DefSubclassProc(h, m, w, l)
}
#[link(name = "uxtheme")]
unsafe extern "system" {
    fn SetWindowTheme(h: HWND, sub: *const u16, list: *const u16) -> i32;
}
pub unsafe fn begin_buffer(
    h: HWND,
    ps: &mut PAINTSTRUCT,
) -> (HDC, (HDC, HBITMAP, HGDIOBJ, i32, i32)) {
    let screen = BeginPaint(h, ps);
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let dc = CreateCompatibleDC(screen);
    let bitmap = CreateCompatibleBitmap(screen, r.right.max(1), r.bottom.max(1));
    let old = SelectObject(dc, bitmap);
    (dc, (screen, bitmap, old, r.right, r.bottom))
}
pub unsafe fn end_buffer(h: HWND, ps: &PAINTSTRUCT, dc: HDC, b: (HDC, HBITMAP, HGDIOBJ, i32, i32)) {
    BitBlt(b.0, 0, 0, b.3, b.4, dc, 0, 0, SRCCOPY);
    SelectObject(dc, b.2);
    DeleteObject(b.1);
    DeleteDC(dc);
    EndPaint(h, ps);
}

pub unsafe fn stable_label(h: HWND) {
    SetWindowSubclass(h, Some(label_proc), 11, 0);
}
unsafe extern "system" fn label_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _: usize,
    _: usize,
) -> isize {
    match m {
        WM_ERASEBKGND => 1,
        WM_SETTEXT => {
            let visible = IsWindowVisible(h) != 0;
            if visible {
                SendMessageW(h, WM_SETREDRAW, 0, 0);
            }
            let v = DefSubclassProc(h, m, w, l);
            if visible {
                SendMessageW(h, WM_SETREDRAW, 1, 0);
                InvalidateRect(h, std::ptr::null(), 0);
            }
            v
        }
        WM_PAINT => {
            if crate::replay_design::draw_note(h) {
                return 0;
            }
            if crate::replay_design::draw_status(h) {
                return 0;
            }
            if crate::replay_design::draw_buffer(h) {
                return 0;
            }
            let mut ps = std::mem::zeroed();
            let (dc, b) = begin_buffer(h, &mut ps);
            if crate::polished::draw_empty(h, dc, style_theme()) {
                end_buffer(h, &ps, dc, b);
                return 0;
            }
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            let mut p = POINT::default();
            let mut bound = RECT::default();
            GetWindowRect(h, &mut bound);
            p.x = bound.left;
            p.y = bound.top;
            ScreenToClient(GetParent(h), &mut p);
            if !GetPropW(
                h,
                "editor-readout\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
            .is_null()
            {
                FillRect(dc, &r, style::brush(bg()));
                let mut words = [0u16; 256];
                let n = GetWindowTextW(h, words.as_mut_ptr(), 256);
                let text = String::from_utf16_lossy(&words[..n as usize]);
                let mut x = 0;
                for word in text.split_whitespace() {
                    let value = word.chars().next().is_some_and(|c| c.is_ascii_digit());
                    let f = style::font(
                        &crate::display::font_name(),
                        16,
                        if value { 600 } else { 400 },
                    );
                    let old = SelectObject(dc, f);
                    let word = format!("{word} ");
                    let w: Vec<u16> = word.encode_utf16().collect();
                    let mut size = SIZE::default();
                    GetTextExtentPoint32W(dc, w.as_ptr(), w.len() as i32, &mut size);
                    SetBkMode(dc, TRANSPARENT as _);
                    SetTextColor(dc, if value { text_color() } else { muted() });
                    TextOutW(dc, x, 3, w.as_ptr(), w.len() as i32);
                    SelectObject(dc, old);
                    x += size.cx + if value { 14 } else { 4 };
                }
                end_buffer(h, &ps, dc, b);
                return 0;
            }
            if !GetPropW(
                h,
                "editor-keycaps\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
            .is_null()
            {
                FillRect(dc, &r, style::brush(bg()));
                let mut x = 0;
                for (word, key) in [
                    ("Click or drag to seek", false),
                    ("<", true),
                    (">", true),
                    ("33 ms", false),
                    ("Shift", true),
                    ("+ arrows 1 s", false),
                    ("[", true),
                    ("set start", false),
                    ("]", true),
                    ("set end", false),
                ] {
                    let f = style::font(
                        &crate::display::font_name(),
                        14,
                        if key { 600 } else { 400 },
                    );
                    let old = SelectObject(dc, f);
                    let words: Vec<u16> = word.encode_utf16().collect();
                    let mut sz = SIZE::default();
                    GetTextExtentPoint32W(dc, words.as_ptr(), words.len() as i32, &mut sz);
                    let width = sz.cx + if key { 14 } else { 6 };
                    if key {
                        rounded(
                            dc,
                            RECT {
                                left: x,
                                top: 0,
                                right: x + width,
                                bottom: 24,
                            },
                            panel(),
                            border(),
                            6,
                        );
                    }
                    SetBkMode(dc, TRANSPARENT as _);
                    SetTextColor(dc, muted());
                    TextOutW(
                        dc,
                        x + if key { 7 } else { 0 },
                        3,
                        words.as_ptr(),
                        words.len() as i32,
                    );
                    SelectObject(dc, old);
                    x += width + 8;
                }
                end_buffer(h, &ps, dc, b);
                return 0;
            }
            let mut parentclass = [0u16; 32];
            GetClassNameW(GetParent(h), parentclass.as_mut_ptr(), 32);
            let editor = String::from_utf16_lossy(&parentclass).starts_with("ZimClipEditor");
            let parent = GetParent(h);
            let mode = GetPropW(
                parent,
                "display-layout\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            ) as usize;
            let viewport =
                String::from_utf16_lossy(&parentclass).starts_with("ZimClipSettingsScroll");
            let settings = !GetPropW(
                parent,
                "display-settings\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
            .is_null();
            let in_panel = if viewport {
                true
            } else if editor {
                (458..650).contains(&p.y)
            } else if settings {
                false
            } else if mode == 3 {
                (100..604).contains(&p.y)
            } else if mode == 1 {
                (100..516
                    + if GetPropW(
                        parent,
                        "compact-expanded\0"
                            .encode_utf16()
                            .collect::<Vec<_>>()
                            .as_ptr(),
                    )
                    .is_null()
                    {
                        0
                    } else {
                        34
                    })
                    .contains(&p.y)
            } else {
                (100..604).contains(&p.y)
            };
            let color = if !GetPropW(h, "card-label\0".encode_utf16().collect::<Vec<_>>().as_ptr()).is_null() {
                panel()
            } else if !GetPropW(
                h,
                "footer-background\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
            .is_null()
            {
                bg()
            } else if p.y
                >= if mode == 1 {
                    590
                } else if mode == 2 {
                    602
                } else {
                    622
                }
            {
                bg()
            } else if !GetPropW(
                parent,
                "surface-bg\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
            )
            .is_null()
            {
                bg()
            } else if in_panel {
                panel()
            } else {
                bg()
            };
            let brush = CreateSolidBrush(color);
            FillRect(dc, &r, brush);
            DeleteObject(brush);
            SetBkMode(dc, TRANSPARENT as _);
            SetTextColor(dc, text_color());
            let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
            let mut text = vec![0u16; (GetWindowTextLengthW(h) + 1).max(1) as usize];
            let len = GetWindowTextW(h, text.as_mut_ptr(), text.len() as i32);
            let status = !GetPropW(
                h,
                "status-single-line\0"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
            .is_null();
            if status {
                for ch in &mut text[..len as usize] {
                    if *ch == 10 || *ch == 13 {
                        *ch = 32;
                    }
                }
            }
            DrawTextW(
                dc,
                text.as_ptr(),
                len,
                &mut r,
                if status {
                    DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX
                } else {
                    DT_LEFT | DT_WORDBREAK | DT_NOPREFIX
                },
            );
            SelectObject(dc, old);
            end_buffer(h, &ps, dc, b);
            0
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(h, Some(label_proc), 11);
            DefSubclassProc(h, m, w, l)
        }
        _ => DefSubclassProc(h, m, w, l),
    }
}

pub unsafe fn replay_scroll(parent: HWND, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let cls: Vec<u16> = "ZimClipLibraryScroll"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let inst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(replay_scroll_proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    CreateWindowExW(
        0,
        cls.as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        w,
        h,
        parent,
        37usize as _,
        inst,
        std::ptr::null(),
    )
}
unsafe extern "system" fn replay_scroll_proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    let parent = GetParent(h);
    let key: Vec<u16> = "library-count".encode_utf16().chain(Some(0)).collect();
    let off: Vec<u16> = "library-offset".encode_utf16().chain(Some(0)).collect();
    let count = GetPropW(parent, key.as_ptr()) as usize;
    let offset = GetPropW(parent, off.as_ptr()) as usize;
    let cap = crate::library_view::capacity_for(parent);
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    match m {
        WM_ERASEBKGND => 1,
        WM_MOUSEWHEEL => SendMessageW(parent, m, w, l),
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP
            if m == WM_LBUTTONDOWN
                || windows_sys::Win32::UI::Input::KeyboardAndMouse::GetCapture() == h =>
        {
            if m == WM_LBUTTONDOWN {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetCapture(h);
            }
            let thumb = (r.bottom * cap as i32 / count.max(cap) as i32).clamp(28, r.bottom);
            let y = ((l >> 16) as i16 as i32 - thumb / 2).clamp(0, (r.bottom - thumb).max(1));
            let value = y as usize * count.saturating_sub(cap) / (r.bottom - thumb).max(1) as usize;
            windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                WM_HOTKEY,
                39,
                value as isize,
            );
            if m == WM_LBUTTONUP {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            0
        }
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let (dc, b) = begin_buffer(h, &mut ps);
            let brush = CreateSolidBrush(panel());
            FillRect(dc, &r, brush);
            DeleteObject(brush);
            if count > cap {
                rounded(dc, r, bg(), bg(), 10);
                let th = (r.bottom * cap as i32 / count as i32).clamp(28, r.bottom);
                let top = (r.bottom - th) * offset as i32 / count.saturating_sub(cap).max(1) as i32;
                rounded(
                    dc,
                    RECT {
                        left: 1,
                        top,
                        right: r.right - 1,
                        bottom: top + th,
                    },
                    accent(),
                    accent(),
                    8,
                );
            }
            end_buffer(h, &ps, dc, b);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

/// Window/tray icon for a non-default identity choice (built-in glyph or uploaded image),
/// or None for the ZimClip default. Cached per (choice, accent); the handle lives for the
/// process, so it is never destroyed while a window still uses it.
pub unsafe fn identity_icon() -> Option<isize> {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<((usize, usize, usize), isize)>> = Mutex::new(None);
    let choice = crate::identity::icon_choice();
    if choice == 0 {
        return None;
    }
    let key = (
        choice,
        ACCENT_INDEX.load(std::sync::atomic::Ordering::Relaxed),
        crate::identity::ICON_GEN.load(std::sync::atomic::Ordering::Relaxed),
    );
    let mut c = CACHE.lock().unwrap();
    if let Some((k, h)) = *c {
        if k == key {
            return Some(h);
        }
    }
    let side = crate::identity::UPLOAD_SIDE as i32;
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = side;
    bi.bmiHeader.biHeight = -side;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let color = CreateDIBSection(std::ptr::null_mut(), &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if color.is_null() || bits.is_null() {
        return None;
    }
    let px = std::slice::from_raw_parts_mut(bits as *mut u8, crate::identity::UPLOAD_BYTES);
    if choice == crate::identity::custom_choice() {
        px.copy_from_slice(&crate::identity::upload_pixels()?);
    } else {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let old = SelectObject(dc, color as _);
        let accent = (accent() & 0xffffff).max(0x010101);
        let ink = if crate::welcome_draw::ink(accent) == 0 { 0x010101 } else { 0xffffff };
        let b = CreateSolidBrush(accent);
        let p = CreatePen(PS_SOLID, 1, accent);
        let (ob, op) = (SelectObject(dc, b as _), SelectObject(dc, p as _));
        RoundRect(dc, 0, 0, side, side, 28, 28);
        SelectObject(dc, ob);
        SelectObject(dc, op);
        DeleteObject(b as _);
        DeleteObject(p as _);
        let pen = CreatePen(PS_SOLID, 4, ink);
        let op = SelectObject(dc, pen as _);
        for stroke in crate::welcome_draw::icon_strokes(choice) {
            let pts: Vec<POINT> = stroke
                .iter()
                .map(|(x, y)| POINT { x: 12 + (x - 12) * 40 / 24 + 20, y: 12 + (y - 12) * 40 / 24 + 20 })
                .collect();
            Polyline(dc, pts.as_ptr(), pts.len() as i32);
        }
        SelectObject(dc, op);
        DeleteObject(pen as _);
        SelectObject(dc, old);
        DeleteDC(dc);
        // GDI leaves alpha at 0: every drawn pixel becomes opaque, the rest stays clear.
        for q in px.chunks_exact_mut(4) {
            if q[0] != 0 || q[1] != 0 || q[2] != 0 {
                q[3] = 255;
            }
        }
    }
    // Explicit all-zero 1bpp mask (rows are 16-bit aligned); alpha comes from the colour bitmap.
    let mask_bits = vec![0u8; (((side as usize + 15) / 16) * 2) * side as usize];
    let mask = CreateBitmap(side, side, 1, 1, mask_bits.as_ptr() as _);
    let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
    let icon = CreateIconIndirect(&info);
    DeleteObject(mask as _);
    DeleteObject(color as _);
    if icon.is_null() {
        return None;
    }
    *c = Some((key, icon as isize));
    Some(icon as isize)
}

pub unsafe fn apply_identity(h: HWND, editor: bool) {
    let name = crate::identity::name();
    let title = if editor {
        format!("{name} | Replay workbench")
    } else {
        format!("{name} {} | Desktop replay", env!("CARGO_PKG_VERSION"))
    };
    let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    SetWindowTextW(h, wide.as_ptr());
    let inst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let id = 2 + ACCENT_INDEX.load(std::sync::atomic::Ordering::Relaxed);
    for (kind, size) in [(ICON_SMALL, 16), (ICON_BIG, 32)] {
        let icon = match identity_icon() {
            Some(i) => i as _,
            None => LoadImageW(inst, id as _, IMAGE_ICON, size, size, LR_SHARED),
        };
        if !icon.is_null() {
            SendMessageW(h, WM_SETICON, kind as usize, icon as isize);
        }
    }
    if !editor {
        crate::background::refresh_identity(h);
    }
}

pub unsafe fn draw_identity_icon(dc: HDC, x: i32, y: i32, size: i32) {
    let inst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let id = 2 + ACCENT_INDEX.load(std::sync::atomic::Ordering::Relaxed);
    let icon = match identity_icon() {
        Some(i) => i as _,
        None => LoadImageW(inst, id as _, IMAGE_ICON, size, size, LR_SHARED),
    };
    if !icon.is_null() {
        DrawIconEx(
            dc,
            x,
            y,
            icon,
            size,
            size,
            0,
            std::ptr::null_mut(),
            DI_NORMAL,
        );
    }
}

// Shared redesign components, additive only. Screen migration is a separate step.
#[allow(dead_code)]
pub mod style;
#[allow(dead_code)]
pub unsafe fn warm_settings() {
    for i in 0..6 {
        let p = theme_colors(i);
        style::brush(p[1]);
        style::pen(p[2], 1);
    }
    for c in accent_colors() {
        style::brush(c);
    }
}
pub fn style_theme() -> style::Theme {
    if crate::polished::enabled() {
        crate::polished::theme(theme_index(), accent_index())
    } else {
        style::Theme::derive(bg(), panel(), border(), text_color(), muted(), accent())
    }
}

pub mod motion;
pub mod transition;

/// Ctrl+A in any single-line or multi-line EDIT of this app selects all its text. The stock
/// control does this on current Windows but not under Wine; doing it here is harmless there.
/// One thread hook, installed once at start, no timers and no per-control work.
pub unsafe fn install_select_all() {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    unsafe extern "system" fn hook(code: i32, w: usize, l: isize) -> isize {
        if code >= 0 && w != 0 {
            let m = &mut *(l as *mut MSG);
            // A bare Ctrl+A arrives as the control character 0x01; Ctrl+Alt (AltGr) does not.
            if m.message == WM_CHAR && m.wParam == 1 {
                let mut cls = [0u16; 8];
                let n = GetClassNameW(m.hwnd, cls.as_mut_ptr(), 8);
                let name = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
                if name.eq_ignore_ascii_case("edit") {
                    SendMessageW(m.hwnd, EM_SETSEL as u32, 0, -1);
                    m.message = WM_NULL;
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }
    // One hook per thread: the editor reopens on the same thread and must not stack hooks.
    thread_local! { static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if INSTALLED.with(|c| c.replace(true)) {
        return;
    }
    SetWindowsHookExW(WH_GETMESSAGE, Some(hook), std::ptr::null_mut(), GetCurrentThreadId());
}
