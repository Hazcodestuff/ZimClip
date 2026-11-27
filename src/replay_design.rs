//! Replay-only migration. Settings/editor wiring stays in its original modules.
use crate::ui::style::{self, Button, State};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Controls::*, Input::KeyboardAndMouse::GetFocus, WindowsAndMessaging::*},
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn prop(h: HWND, k: &str) -> usize {
    GetPropW(h, wide(k).as_ptr()) as usize
}
unsafe fn slot(h: HWND, k: &str) -> HWND {
    prop(h, &format!("slot-{k}")) as _
}
unsafe fn put(h: HWND, x: i32, y: i32, w: i32, z: i32) {
    if !h.is_null() {
        let mut r = RECT::default();
        GetWindowRect(h, &mut r);
        let mut p = POINT {
            x: r.left,
            y: r.top,
        };
        ScreenToClient(GetParent(h), &mut p);
        if p.x != x || p.y != y || r.right - r.left != w || r.bottom - r.top != z {
            crate::display::put(h, x, y, w, z);
        }
    }
}
unsafe fn show(h: HWND, on: bool) {
    if !h.is_null() {
        let visible_style = GetWindowLongW(h, GWL_STYLE) as u32 & WS_VISIBLE != 0;
        if visible_style != on {
            ShowWindow(h, if on { SW_SHOW } else { SW_HIDE });
        }
    }
}
unsafe fn text(h: HWND) -> String {
    let mut b = vec![0u16; (GetWindowTextLengthW(h) + 1).max(1) as usize];
    let n = GetWindowTextW(h, b.as_mut_ptr(), b.len() as _);
    String::from_utf16_lossy(&b[..n as usize])
}
pub unsafe fn prepare(h: HWND, f: HFONT) {
    style::warm(crate::ui::style_theme(), &crate::display::font_name(), 1.);
    for id in [1, 2, 4, 5, 7, 12, 10, 43, 40, 44, 14, 70, 71, 81, 85] {
        SetPropW(
            GetDlgItem(h, id),
            wide("redesign-control").as_ptr(),
            1usize as _,
        );
    }
    SetPropW(
        slot(h, "source-note"),
        wide("surface-bg").as_ptr(),
        std::ptr::null_mut(),
    );
    let _ = f;
}
pub unsafe fn confirm_clear(h: HWND) -> bool {
    let mut font = SendMessageW(h, WM_GETFONT, 0, 0) as HFONT;
    if font.is_null() {
        font = GetStockObject(DEFAULT_GUI_FONT) as HFONT;
    }
    crate::editor::confirm_restart(h, font)
}
pub unsafe fn source_state(h: HWND, on: bool, description: &str) {
    let changed = prop(h, "source-on") != on as usize;
    SetPropW(h, wide("source-on").as_ptr(), on as usize as _);
    if changed {
        InvalidateRect(h, std::ptr::null(), 0);
    }
    let caption = if windows_sys::Win32::UI::WindowsAndMessaging::GetDlgCtrlID(h) == 7 {
        "System audio"
    } else {
        "Microphone"
    };
    let value = format!("{caption}\n{description}");
    if text(h) != value {
        SetWindowTextW(h, wide(&value).as_ptr());
    }
}
pub unsafe fn buffer(h: HWND, filled: usize, total: usize) {
    let value = format!("{filled}/{total}");
    if text(h) != value {
        SetWindowTextW(h, wide(&value).as_ptr());
    }
}
pub fn boxes(mode: usize, settings: bool) -> Vec<(i32, i32, i32, i32)> {
    if settings {
        return vec![];
    }
    match mode {
        0 => vec![(16, 88, 474, 622), (490, 88, 948, 622)],
        1 => vec![(16, 88, 588, 356), (16, 372, 588, 590)],
        2 => vec![(16, 88, 380, 602), (396, 88, 1218, 602)],
        _ => vec![(16, 88, 948, 334), (16, 350, 948, 622)],
    }
}
/// Legacy card boxes for the real window. Legacy Compact is paged like Polished Compact: one card
/// per page (Home controls, or the Clips library), not two stacked cards that cannot hold the
/// controls drawn over them. Other layouts use `boxes`.
pub fn boxes_for(mode: usize, settings: bool, client_bottom: i32) -> Vec<(i32, i32, i32, i32)> {
    match crate::nav::compact_page(mode, false, settings) {
        Some(1) => vec![(16, 88, if mode == 3 { 948 } else { 588 }, crate::layout_plan::content_bottom(client_bottom).max(300))],
        Some(_) => vec![(16, 88, if mode == 3 { 948 } else { 588 }, if mode == 3 { 372 } else { 540 })],
        None => boxes(mode, settings),
    }
}
pub unsafe fn draw_note(h: HWND) -> bool {
    if prop(h, "replay-note") == 0 || prop(GetParent(h), "display-settings") != 0 {
        return false;
    }
    let mut ps = PAINTSTRUCT::default();
    let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let t = crate::ui::style_theme();
    if crate::polished::draw_note(h, dc, t) {
        crate::ui::end_buffer(h, &ps, dc, b);
        return true;
    }
    FillRect(dc, &r, style::brush(t.panel));
    let original = text(h);
    let note = if r.right - r.left < 380 {
        "Source changes clear the unsaved replay."
    } else {
        &original
    };
    style::note(
        dc,
        r,
        note,
        false,
        t,
        style::font(&crate::display::font_name(), 12, 400),
    );
    crate::ui::end_buffer(h, &ps, dc, b);
    true
}
pub unsafe fn draw_status(h: HWND) -> bool {
    if prop(h, "replay-status") == 0 || prop(GetParent(h), "display-settings") != 0 {
        return false;
    }
    let mut ps = PAINTSTRUCT::default();
    let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let t = crate::ui::style_theme();
    if crate::polished::draw_status(h, dc, t) {
        crate::ui::end_buffer(h, &ps, dc, b);
        return true;
    }
    FillRect(dc, &r, style::brush(t.panel));
    let value = text(h);
    let color =
        if value.contains("error") || value.contains("failed") || value.contains("Recording") {
            t.rec
        } else if value.contains("paused") {
            t.amber
        } else {
            t.accent
        };
    style::rect(dc, r, t.raise, t.border, 10);
    style::disc(dc, RECT { left: 8, top: 9, right: 14, bottom: 15 }, color, None);
    let mut textrect = r;
    textrect.left += 21;
    textrect.right -= 5;
    style::label(
        dc,
        &value,
        textrect,
        style::font(&crate::display::font_name(), 12, 400),
        t.text,
        DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    crate::ui::end_buffer(h, &ps, dc, b);
    true
}
pub unsafe fn draw_buffer(h: HWND) -> bool {
    if prop(h, "replay-buffer") == 0 {
        return false;
    }
    let mut ps = PAINTSTRUCT::default();
    let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
    let t = crate::ui::style_theme();
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    if crate::polished::draw_buffer(h, dc, t) {
        crate::ui::end_buffer(h, &ps, dc, b);
        return true;
    }
    FillRect(dc, &r, style::brush(t.panel));
    let value = text(h);
    let (filled, total) = value
        .split_once('/')
        .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
        .unwrap_or((0, 30));
    let n = total.clamp(1, 45);
    let w = (r.right / n as i32).max(2);
    for i in 0..n {
        let c = if i * total / n < filled {
            t.accent
        } else {
            t.border
        };
        style::rect(
            dc,
            RECT {
                left: i as i32 * w,
                top: 2,
                right: (i as i32 + 1) * w - 2,
                bottom: 22,
            },
            c,
            c,
            2,
        );
    }
    let f = style::font(&crate::display::font_name(), 12, 400);
    style::label(
        dc,
        &format!("{total}s ago"),
        RECT {
            left: 0,
            top: 26,
            right: r.right / 2,
            bottom: 43,
        },
        f,
        t.muted,
        DT_SINGLELINE,
    );
    style::label(
        dc,
        "now",
        RECT {
            left: r.right / 2,
            top: 26,
            right: r.right,
            bottom: 43,
        },
        f,
        t.muted,
        DT_SINGLELINE | DT_RIGHT,
    );
    crate::ui::end_buffer(h, &ps, dc, b);
    true
}
pub unsafe fn draw_control(d: &DRAWITEMSTRUCT) -> bool {
    if crate::polished::draw_control(d) {
        return true;
    }
    if d.CtlType != ODT_BUTTON || prop(d.hwndItem, "redesign-control") == 0 {
        return false;
    }
    let parent = GetParent(d.hwndItem);
    let setting = prop(parent, "display-settings") != 0;
    let source = d.CtlID == 7 || d.CtlID == 12;
    if setting || crate::display::font_name().is_empty() {
        return false;
    }
    let mut t = crate::ui::style_theme();
    let disabled = d.itemState & ODS_DISABLED != 0;
    let mut pt = POINT::default();
    GetCursorPos(&mut pt);
    let mut bound = RECT::default();
    GetWindowRect(d.hwndItem, &mut bound);
    let s = State {
        hover: !disabled
            && pt.x >= bound.left
            && pt.x < bound.right
            && pt.y >= bound.top
            && pt.y < bound.bottom,
        pressed: d.itemState & ODS_SELECTED != 0,
        focused: GetFocus() == d.hwndItem,
        disabled,
        ..Default::default()
    };
    let motion = crate::ui::motion::sample(
        d.hwndItem,
        s.hover,
        s.pressed,
        prop(d.hwndItem, "source-on") != 0,
    );
    // Quantize paint colours so the shared GDI cache remains bounded.
    let hover = (motion[0] * 12.).round() as u32;
    t.hover = style::mix(t.raise, t.hover, hover * 100 / 12);
    t.accent = style::mix(t.accent, 0xffffff, hover * 12 / 12);
    let f = style::font(&crate::display::font_name(), 14, 400);
    let title = text(d.hwndItem);
    FillRect(d.hDC, &d.rcItem, style::brush(t.panel));
    if source {
        style::rect(
            d.hDC,
            d.rcItem,
            t.hover,
            if s.focused { t.accent } else { t.border },
            12,
        );
        let mut rr = d.rcItem;
        rr.left += 12;
        rr.right -= 62;
        rr.top += 7;
        rr.bottom = rr.top + 20;
        let (name, desc) = title.split_once('\n').unwrap_or((&title, ""));
        let desc = if d.rcItem.right - d.rcItem.left < 280 {
            if prop(d.hwndItem, "source-on") != 0 {
                "Enabled"
            } else {
                "Off, not recorded"
            }
        } else {
            desc
        };
        style::label(d.hDC, name, rr, f, t.text, DT_SINGLELINE | DT_END_ELLIPSIS);
        if d.rcItem.bottom - d.rcItem.top < 50 {
            rr.top -= 6;
            rr.bottom -= 6;
        }
        rr.top += 22;
        rr.bottom += 22;
        if d.rcItem.bottom - d.rcItem.top >= 50 {
            style::label(
                d.hDC,
                desc,
                rr,
                style::font(&crate::display::font_name(), 12, 400),
                t.muted,
                DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        }
        style::switch_at(
            d.hDC,
            d.rcItem,
            motion[2],
            State {
                focused: false,
                ..s
            },
            t,
        );
        if s.focused {
            style::focus(d.hDC, d.rcItem, t)
        }
    } else {
        let kind = match d.CtlID {
            1 => Button::Save,
            2 => Button::Primary,
            4 | 5 | 40 => Button::Default,
            43 => Button::Record,
            71 => Button::Danger,
            _ => Button::Default,
        };
        let compact_record = if d.CtlID == 10 && d.rcItem.right - d.rcItem.left < 90 {
            if title.starts_with("Resume") {
                "Resume".into()
            } else {
                "Pause".into()
            }
        } else if d.CtlID == 43 && d.rcItem.right - d.rcItem.left < 180 {
            if title.starts_with("Stop recording") {
                title.replace("Stop recording", "Stop")
            } else {
                "Record".into()
            }
        } else {
            title.clone()
        };
        let name = if d.CtlID == 1 && disabled {
            "Waiting for capture..."
        } else {
            &compact_record
        };
        let mut state = s;
        state.hover = motion[0] > 0.01;
        state.pressed = motion[1] > 0.5;
        state.selected = d.CtlID == 43 && title.starts_with("Stop recording");
        if d.CtlID == 1 && !disabled {
            let (caption, key) = name.split_once("   ").unwrap_or((name, ""));
            style::button(d.hDC, d.rcItem, "", kind, state, t, f);
            let mut r = d.rcItem;
            r.left += 12;
            r.right -= 100;
            style::label(
                d.hDC,
                caption,
                r,
                f,
                t.ink,
                DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_END_ELLIPSIS,
            );
            let cap = RECT {
                left: d.rcItem.right - 90,
                top: d.rcItem.top + 14,
                right: d.rcItem.right - 14,
                bottom: d.rcItem.bottom - 14,
            };
            style::keycap(
                d.hDC,
                cap,
                key,
                t,
                style::font(&crate::display::font_name(), 12, 400),
            );
        } else {
            style::button(d.hDC, d.rcItem, name, kind, state, t, f);
        }
    }
    true
}
/// Sidebar Home recent cards (ids 120..=123) and the "View all" link (125). Shown only on
/// the wide Sidebar Home page; hidden everywhere else, including Settings.
pub unsafe fn home_rows(h: HWND) {
    let mut client = RECT::default();
    GetClientRect(h, &mut client);
    let settings = prop(h, "display-settings") != 0;
    let on = !settings
        && crate::polished::enabled()
        && crate::polished::sidebar()
        && crate::nav::visible(crate::polished::nav_page()).home
        && crate::nav::home_wide(client.right);
    let (x, w) = crate::nav::home_panel(client.right);
    let heading = crate::nav::panel_heading_y(w);
    let count = crate::home_stats::info().recent.len().min(4);
    let step = 62;
    let top = heading + 32;
    let fit = (((client.bottom - 80 - top) / step).max(0)) as usize;
    for i in 0..4usize {
        let c = GetDlgItem(h, 120 + i as i32);
        if c.is_null() {
            continue;
        }
        let shown = on && i < count && i < fit;
        if shown {
            put(c, x, top + i as i32 * step, w, 56);
        }
        show(c, shown);
    }
    let link = GetDlgItem(h, 125);
    if !link.is_null() {
        if on {
            put(link, x + w - 80, heading, 80, 24);
        }
        show(link, on && count > 0);
    }
}
pub unsafe fn layout(
    h: HWND,
    mode: usize,
    settings: bool,
    expanded: bool,
    rows: &[HWND],
    large: bool,
) {
    let mut client = RECT::default();
    GetClientRect(h, &mut client);
    let bottom = client.bottom;
    // Sidebar layout: Balanced geometry, one page at a time, navigation rail on the left.
    let sidebar = crate::polished::sidebar() && !settings && mode == 4;
    let orig_mode = mode;
    let mode = if sidebar { 0 } else { mode };
    crate::nav::set_stacked_short(client.bottom < crate::layout_plan::LEGACY_STACKED_SIDE_BY_SIDE_MIN_H);
    let compact_paged = crate::nav::compact_page(orig_mode, crate::polished::enabled(), settings).is_some();
    for id in 110..=113usize {
        let b = GetDlgItem(h, id as i32);
        // Compact: only the Home and Clips page buttons, as small icon buttons under the app name.
        // Editor (112) and Settings (113) rail entries are retired: opening a clip opens the editor,
        // and Settings is the button at the top right. The ids stay allocated and hidden.
        show(b, (sidebar && id <= 111) || (compact_paged && id <= 111));
        if sidebar {
            put(b, 12, 96 + (id as i32 - 110) * 46, crate::polished::SB - 24, 38);
        } else if compact_paged && id <= 111 {
            if crate::polished::enabled() {
                put(b, 76 + (id as i32 - 110) * 64, 48, 56, 32);
            } else {
                // Legacy shows the words, so the buttons are wide enough for "Home" and "Clips".
                put(b, 76 + (id as i32 - 110) * 80, 48, 72, 32);
            }
        }
        if compact_paged && id <= 111 && !crate::polished::enabled() {
            // Legacy has no icon style: the current page uses the same "active" fill as the
            // Settings section buttons.
            let on = crate::nav::nav_page() == id - 110;
            SetPropW(b, wide("section-active").as_ptr(), on as usize as _);
            InvalidateRect(b, std::ptr::null(), 1);
        }
    }
    {
        let arrange = GetDlgItem(h, 114);
        show(arrange, sidebar && crate::nav::visible(crate::polished::nav_page()).home);
        if sidebar {
            put(arrange, client.right - 338, 18, 38, 38);
        }
    }
    for key in ["buffer", "sources", "source-note", "encoder"] {
        show(slot(h, key), !settings)
    }
    home_rows(h);
    if settings {
        let view = GetDlgItem(h, 74);
        for id in [7, 12, 73] {
            let c = GetDlgItem(h, id);
            if !c.is_null() {
                SetParent(c, view);
            }
        }
        // The Home sub-page switch belongs to Sidebar Home only; Settings never shows it.
        show(GetDlgItem(h, crate::ctl_ids::HOME_SUBPAGE as i32), false);
        crate::settings_view::layout(h, true);
        return;
    }
    // Keep existing quick-button IDs allocated and wired, but use the original
    // System/Mic controls in Replay. They remain reparented Settings controls later.
    show(GetDlgItem(h, 86), false);
    show(GetDlgItem(h, 87), false);
    for id in [7, 12, 73] {
        let mut c = GetDlgItem(h, id);
        if c.is_null() {
            c = GetDlgItem(GetDlgItem(h, 74), id);
        }
        SetParent(c, h);
        SetPropW(c, wide("redesign-control").as_ptr(), 1usize as _);
        show(c, !expanded);
    }
    // The header label is opaque; keep it clear of the two top-right buttons.
    let heading_width = (client.right - 384).max(200);
    let hx = if sidebar { crate::polished::SB + 24 } else { 76 };
    let heading_width = if sidebar { (heading_width - (hx - 76) - 140).max(200) } else { heading_width };
    put(slot(h, "heading"), hx, 12, heading_width, 32);
    put(slot(h, "tagline"), hx, 48, heading_width, 24);
    if compact_paged {
        // The page icons take the tagline's place, so the compact header stays uncrowded.
        show(slot(h, "tagline"), false);
    }
    put(GetDlgItem(h, 14), client.right - 154, 18, 130, 38);
    put(GetDlgItem(h, 89), client.right - 292, 18, 130, 38);
    // Reserve a stable recovery-action slot so late visibility cannot cover text.
    let result_width = if mode == 1 {
        client.right - 48
    } else {
        client.right - 510
    };
    put(
        slot(h, "result"),
        24,
        bottom - if mode == 1 { 98 } else { 70 },
        result_width.max(80),
        38,
    );
    put(slot(h, "footer"), 24, bottom - 29, client.right - 48, 22);
    put(GetDlgItem(h, 4), client.right - 300, bottom - 66, 120, 32);
    put(GetDlgItem(h, 5), client.right - 168, bottom - 66, 144, 32);
    let (x, w) = match mode {
        0 if sidebar => crate::nav::home_col(client.right),
        0 => (34, 422),
        1 => (34, 536),
        2 => (34, 328),
        _ => (34, 422),
    };
    if mode < 3 {
        put(slot(h, "viewheading"), x, 104, 170, 20);
        put(slot(h, "status"), x + w - 150, 104, 150, 24);
        put(slot(h, "simplehint"), x, 140, w, 56);
        put(slot(h, "encoder"), x, 201, w, 24);
        put(slot(h, "buffer"), x, 230, w, 44);
        put(GetDlgItem(h, 1), x, 282, w, 52);
        put(slot(h, "sources"), x, 344, w, 20);
        put(GetDlgItem(h, 7), x, 370, w, 46);
        put(GetDlgItem(h, 12), x, 424, w, 46);
        put(GetDlgItem(h, 73), x, 478, w, 46);
        put(slot(h, "source-note"), x, 534, w, 34);
        put(GetDlgItem(h, 10), x, 576, (w - 128) / 2, 36);
        put(
            GetDlgItem(h, 43),
            x + (w - 128) / 2 + 8,
            576,
            (w - 128) / 2,
            36,
        );
        put(GetDlgItem(h, 40), x + w - 104, 576, 104, 36);
        put(slot(h, "warning"), x, 612, w, 24);
        show(slot(h, "warning"), false);
        show(slot(h, "privacy"), false);
        put(GetDlgItem(h, 44), x, 612, w, 24);
        if mode == 2 {
            put(slot(h, "simplehint"), x, 138, w, 56);
            put(slot(h, "encoder"), x, 196, w, 24);
            put(slot(h, "buffer"), x, 222, w, 44);
            put(GetDlgItem(h, 1), x, 270, w, 48);
            put(slot(h, "sources"), x, 328, w, 20);
            put(GetDlgItem(h, 7), x, 354, w, 46);
            put(GetDlgItem(h, 12), x, 406, w, 46);
            put(GetDlgItem(h, 73), x, 458, w, 46);
            put(slot(h, "source-note"), x, 512, w, 34);
            put(GetDlgItem(h, 10), x, 552, 100, 36);
            put(GetDlgItem(h, 43), x + 108, 552, 116, 36);
            put(GetDlgItem(h, 40), x + 232, 552, 96, 36);
            put(GetDlgItem(h, 44), x, 590, w, 24);
        }
        if mode == 1 {
            put(GetDlgItem(h, 7), x, 370, 260, 54);
            put(GetDlgItem(h, 12), x + 276, 370, 260, 54);
            put(slot(h, "source-note"), x, 440, w, 38);
            put(GetDlgItem(h, 10), x, 490, 260, 38);
            put(GetDlgItem(h, 43), x + 276, 490, 260, 38);
            put(GetDlgItem(h, 40), x, 542, 124, 34);
            put(slot(h, "open"), x + 140, 542, w - 140, 34);
            put(GetDlgItem(h, 44), x, 584, w, 26);
        }
        if !sidebar && !expanded && (mode == 0 || mode == 2) {
            // Home column by tier: the established layout when it fits above the footer band,
            // then the optional hint, then the sources heading and note, are dropped. This is
            // the one writer for these controls' position and the hint/heading/note visibility.
            let hr = crate::layout_plan::home_rects(mode == 2, x, w, crate::layout_plan::content_bottom(client.bottom));
            let at = |r: crate::layout_plan::R| (r.0, r.1, r.2, r.3);
            let pl = |c: HWND, r: crate::layout_plan::R| { let r = at(r); put(c, r.0, r.1, r.2, r.3) };
            pl(slot(h, "encoder"), hr.encoder);
            pl(slot(h, "buffer"), hr.buffer);
            pl(GetDlgItem(h, 1), hr.save);
            pl(GetDlgItem(h, 7), hr.sys);
            pl(GetDlgItem(h, 12), hr.mic);
            pl(GetDlgItem(h, 73), hr.voice);
            pl(GetDlgItem(h, 10), hr.pause);
            pl(GetDlgItem(h, 43), hr.record);
            pl(GetDlgItem(h, 40), hr.details);
            if let Some(r) = hr.hint { pl(slot(h, "simplehint"), r); }
            if let Some(r) = hr.sources { pl(slot(h, "sources"), r); }
            if let Some(r) = hr.note { pl(slot(h, "source-note"), r); }
            show(slot(h, "simplehint"), hr.hint.is_some());
            show(slot(h, "sources"), hr.sources.is_some());
            show(slot(h, "source-note"), hr.note.is_some());
        }
        if expanded {
            show(slot(h, "sources"), false);
            put(slot(h, "details"), x, 358, w, 78);
            put(slot(h, "audio"), x, 442, w, 100);
            if mode == 0 || mode == 2 {
                // Expanded Home by tier, same footer band, Sidebar included (it maps to mode 0 geometry). Declared floor: EXPANDED_FLOOR_H.
                let e = crate::layout_plan::expanded_rects(mode == 2, x, w, crate::layout_plan::content_bottom(client.bottom));
                let pl = |c: HWND, r: crate::layout_plan::R| put(c, r.0, r.1, r.2, r.3);
                pl(slot(h, "encoder"), e.encoder);
                pl(slot(h, "buffer"), e.buffer);
                pl(GetDlgItem(h, 1), e.save);
                pl(slot(h, "details"), e.details_text);
                pl(slot(h, "audio"), e.audio_text);
                pl(GetDlgItem(h, 10), e.pause);
                pl(GetDlgItem(h, 43), e.record);
                pl(GetDlgItem(h, 40), e.details_button);
                // The label follows the width the plan gave the button: "Hide details" needs 128 px.
                SetWindowTextW(GetDlgItem(h, 40), wide(crate::layout_plan::details_label(true, e.details_button.2)).as_ptr());
            }
        }
        if mode != 1 {
            let (rx, rw) = if crate::polished::enabled() {
                if sidebar {
                    crate::nav::library_col(client.right)
                } else if mode == 0 {
                    (508, (client.right - 542).max(300))
                } else {
                    (414, (client.right - 448).max(300))
                }
            } else if mode == 0 {
                (508, 422)
            } else {
                (414, 786)
            };
            show(slot(h, "rightheading"), true);
            put(slot(h, "rightheading"), rx, 104, 210, 20);
            let pair_w = if crate::polished::enabled() { 36 } else { 100 };
            put(slot(h, "righthint"), rx, 130, rw - (2 * pair_w + 16).max(110), 24);
            put(GetDlgItem(h, 90), rx + rw - 2 * pair_w, 102, pair_w, 30);
            put(GetDlgItem(h, 36), rx + rw - pair_w, 102, pair_w, 30);
            put(GetDlgItem(h, 76), rx, 164, rw, 32);
            put(GetDlgItem(h, 77), rx, 204, (rw - 12) / 2, 32);
            put(
                GetDlgItem(h, 78),
                rx + (rw + 12) / 2,
                204,
                (rw - 12) / 2,
                32,
            );
            for (i, row) in rows.iter().enumerate() {
                put(
                    *row,
                    rx,
                    246 + i as i32 * if large { 62 } else { 54 },
                    rw - 16,
                    if large { 58 } else { 50 },
                );
            }
            put(GetDlgItem(h, 37), rx + rw - 8, 246, 6, 304);
            let actiony = if mode == 0 { 576 } else { 556 };
            let selecting = crate::library::state().lock().unwrap().active;
            show(slot(h, "open"), !selecting);
            show(GetDlgItem(h, 71), selecting);
            put(
                if selecting {
                    GetDlgItem(h, 71)
                } else {
                    slot(h, "open")
                },
                rx,
                actiony,
                (rw - 308).max(80),
                36,
            );
            put(GetDlgItem(h, 70), rx + rw - 300, actiony, 88, 36);
            put(GetDlgItem(h, 81), rx + rw - 204, actiony, 100, 36);
            put(GetDlgItem(h, 85), rx + rw - 96, actiony, 80, 36);
        }
    } else {
        put(slot(h, "viewheading"), 34, 104, 160, 20);
        put(slot(h, "status"), 214, 104, 240, 24);
        put(slot(h, "simplehint"), 34, 138, 422, 56);
        put(slot(h, "encoder"), 34, 198, 422, 24);
        put(slot(h, "buffer"), 34, 230, 422, 44);
        // Stacked: the controls column uses the card's full remaining width, so the source rows
        // keep their names and descriptions instead of cutting them to "Syste...".
        let (cx, cw) = (490, (client.right - 34 - 490).max(422));
        let half = (cw - 12) / 2;
        put(GetDlgItem(h, 1), cx, 140, cw, 52);
        put(GetDlgItem(h, 7), cx, 204, half, 54);
        put(GetDlgItem(h, 12), cx + half + 12, 204, half, 54);
        put(GetDlgItem(h, 73), cx, 268, half, 42);
        let third = (half - 8) / 2;
        put(GetDlgItem(h, 10), cx + half + 12, 268, third, 42);
        put(GetDlgItem(h, 43), cx + half + 12 + third + 8, 268, third, 42);
        put(GetDlgItem(h, 40), 34, 282, 146, 36);
        put(slot(h, "source-note"), 190, 282, 268, 40);
        show(slot(h, "sources"), false);
        show(slot(h, "privacy"), false);
        show(slot(h, "warning"), false);
        put(GetDlgItem(h, 44), 490, 318, cw, 30);
        if expanded {
            // Expanded Stacked: status line first (the warning follows the encoder slot),
            // then the details block, the audio block, and Details at the bottom of the left column.
            put(slot(h, "encoder"), 34, 136, 422, 24);
            put(slot(h, "details"), 34, 168, 422, 62);
            put(slot(h, "audio"), 34, 234, 422, 80);
            put(GetDlgItem(h, 40), 34, 322, 146, 36);
            show(slot(h, "simplehint"), false);
            show(slot(h, "encoder"), false);
            show(slot(h, "buffer"), false);
        }
    }
    put(GetDlgItem(h, 44), client.right - 470, bottom - 66, 160, 32);
    home_rows(h);
    if sidebar && !expanded {
        // Home cards in the saved order, stacked from the Buffer row's original y.
        // Short client: the whole column moves up by the hint's height (hint dropped), the same
        // first tier as Balanced. Below SIDEBAR_FLOOR_H the cards still overlap the footer band.
        let cb = crate::layout_plan::content_bottom(client.bottom);
        let split = crate::layout_plan::sidebar_needs_split(cb);
        if !split {
            crate::nav::set_home_sub(0);
        }
        let sdy = crate::layout_plan::sidebar_shift(cb);
        if split {
            let sub = crate::nav::home_sub_for(&crate::nav::cards());
            show(slot(h, "simplehint"), false);
            put(slot(h, "encoder"), x, crate::layout_plan::SPLIT_ENCODER_Y, w, 24);
            let sw = GetDlgItem(h, crate::ctl_ids::HOME_SUBPAGE as i32);
            if sub == 0 {
                // Capture: encoder, non-Sources cards in saved order, switch, action row.
                let mut y = crate::layout_plan::SPLIT_FLOW_TOP;
                for (card, _) in crate::nav::cards().flow(0) {
                    match card {
                        crate::nav::Card::Buffer => { put(slot(h, "buffer"), x, y, w, 44); y += 52; }
                        crate::nav::Card::Save => { put(GetDlgItem(h, 1), x, y, w, 52); y += 60; }
                        crate::nav::Card::Sources => {}
                    }
                }
                let ry = crate::layout_plan::split_row_y(cb);
                let half = (w - 128) / 2;
                put(GetDlgItem(h, 10), x, ry, half, 36);
                put(GetDlgItem(h, 43), x + half + 8, ry, half, 36);
                put(GetDlgItem(h, 40), x + w - 104, ry, 104, 36);
                put(sw, x, crate::layout_plan::split_switch_y_capture(cb), w, 36);
            } else {
                // Sources: the card at the top of the column, switch back at the action row.
                let y = crate::layout_plan::SPLIT_ENCODER_Y + 32;
                let s = crate::layout_plan::sources_rects(cb);
                put(slot(h, "sources"), x, y, w, 20);
                for (id, ry) in [7, 12, 73].into_iter().zip(s.rows_y) {
                    put(GetDlgItem(h, id), x, y + ry, w, s.row_h);
                }
                if let Some((ny, nh)) = s.note {
                    put(slot(h, "source-note"), x, y + ny, w, nh);
                }
                put(sw, x, crate::layout_plan::split_row_y(cb), w, 36);
            }
        } else if sdy != 0 {
            show(slot(h, "simplehint"), false);
            put(slot(h, "encoder"), x, 201 + sdy, w, 24);
            let half = (w - 128) / 2;
            put(GetDlgItem(h, 10), x, 576 + sdy, half, 36);
            put(GetDlgItem(h, 43), x + half + 8, 576 + sdy, half, 36);
            put(GetDlgItem(h, 40), x + w - 104, 576 + sdy, 104, 36);
        } else {
            show(slot(h, "simplehint"), true);
        }
        for (card, y) in crate::nav::cards().flow(230 + sdy).into_iter().filter(|_| !split) {
            match card {
                crate::nav::Card::Buffer => put(slot(h, "buffer"), x, y, w, 44),
                crate::nav::Card::Save => put(GetDlgItem(h, 1), x, y, w, 52),
                crate::nav::Card::Sources => {
                    put(slot(h, "sources"), x, y, w, 20);
                    put(GetDlgItem(h, 7), x, y + 26, w, 46);
                    put(GetDlgItem(h, 12), x, y + 80, w, 46);
                    put(GetDlgItem(h, 73), x, y + 134, w, 46);
                    put(slot(h, "source-note"), x, y + 190, w, 34);
                }
            }
        }
    }
    crate::library_view::layout(h, orig_mode, rows, large);
    attention(h);
    if sidebar {
        sidebar_pages(h);
    }
    // Native tab order follows this migrated visual order; hidden controls skipped.
    let order = [
        110, 111, 112, 113, 114, crate::ctl_ids::HOME_SUBPAGE as i32, 14, 89, 1, 7, 12, 73, 10, 43, 40, 76, 77, 78, 36, 90, 100, 101, 102, 103, 104, 2, 70, 81,
        85, 4, 5,
    ];
    let mut prev = std::ptr::null_mut();
    for id in order {
        let c = GetDlgItem(h, id);
        if !c.is_null() {
            SetWindowPos(
                c,
                prev,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            prev = c;
        }
    }
}

pub(crate) const HOME_IDS: [i32; 10] = [1, 7, 12, 73, 10, 43, 40, 44, 114, crate::ctl_ids::HOME_SUBPAGE as i32];
pub(crate) const HOME_SLOTS: [&str; 11] = [
    "viewheading", "status", "simplehint", "encoder", "buffer", "sources", "source-note", "warning",
    "details", "audio", "privacy",
];
pub(crate) const CLIPS_IDS: [i32; 15] = [76, 77, 78, 36, 90, 37, 70, 81, 85, 71, 100, 101, 102, 103, 104];
pub(crate) const CLIPS_SLOTS: [&str; 3] = ["rightheading", "righthint", "open"];

/// Home page shows the capture column; the library controls are hidden.
pub unsafe fn sidebar_hide_library(h: HWND) {
    if !crate::polished::sidebar() || prop(h, "display-settings") != 0 || !crate::nav::visible(crate::polished::nav_page()).home {
        return;
    }
    for id in CLIPS_IDS {
        show(GetDlgItem(h, id), false);
    }
    for k in CLIPS_SLOTS {
        show(slot(h, k), false);
    }
}
/// Sidebar pages. Called last in layout, after attention(), so it has the final say on
/// visibility. Clips hides the capture column; Home hides the library and re-shows the
/// capture controls that a previous Clips visit hid.
pub unsafe fn sidebar_pages(h: HWND) {
    if !crate::polished::sidebar() || prop(h, "display-settings") != 0 {
        return;
    }
    if crate::nav::visible(crate::polished::nav_page()).home {
        sidebar_hide_library(h);
        show_home(h, true);
    } else {
        hide_home(h);
    }
}

/// Shared Home-page visibility for every paged layout (Sidebar, Compact): one rule set
/// for expanded details, hidden cards and the recover button. `arrange` pins button 114.
pub(crate) unsafe fn show_home(h: HWND, arrange: bool) {
    let expanded = prop(h, "compact-expanded") != 0;
    // Arrange Home exists only in Sidebar, so hidden cards apply only there; other layouts
    // always show every card and the user cannot be left without a way to bring one back.
    let cards = if arrange { crate::nav::cards() } else { Default::default() };
    // Split Sidebar Home: decided from the live client height by the same pure rule the layout used.
    let split = arrange && !expanded && {
        let mut cr = RECT::default();
        GetClientRect(h, &mut cr);
        crate::layout_plan::sidebar_needs_split(crate::layout_plan::content_bottom(cr.bottom))
    };
    let sub = split.then(|| crate::nav::home_sub_for(&cards));
    let sh = crate::nav::home_show_sub(expanded, &cards, sub);
    for (id, on) in crate::nav::home_buttons(crate::nav::recover_pending()) {
        if id == 114 && !arrange {
            continue;
        }
        // Pause, Record and Details belong to the Capture sub-page; Save follows its card flag.
        let on = on && !(sub == Some(1) && matches!(id, 1 | 10 | 43 | 40)) && !(id == 1 && !sh.save);
        show(GetDlgItem(h, id), on);
    }
    {
        let sw = GetDlgItem(h, crate::ctl_ids::HOME_SUBPAGE as i32);
        // No Sources page, no switch, when the user has hidden the Sources card.
        let has_page = arrange && !crate::nav::cards().is_hidden(crate::nav::Card::Sources);
        show(sw, split && has_page);
        if split {
            SetWindowTextW(sw, wide(if sub == Some(0) { "Audio sources  >" } else { "<  Capture" }).as_ptr());
        }
    }
    for k in ["viewheading", "status"] {
        show(slot(h, k), true);
    }
    show(slot(h, "buffer"), sh.buffer);
    // The tight split Sources page has no note (the one visibility owner decides it here).
    let note_ok = !split || {
        let mut cr = RECT::default();
        GetClientRect(h, &mut cr);
        crate::layout_plan::sources_rects(crate::layout_plan::content_bottom(cr.bottom)).note.is_some()
    };
    show(slot(h, "sources"), sh.sources);
    show(slot(h, "source-note"), sh.sources && note_ok);
    // Single owner of the hint's visibility: the Sidebar short shift drops it here, so no later
    // writer can bring it back over the shifted column.
    let sidebar_short = arrange && !expanded && {
        let mut cr = RECT::default();
        GetClientRect(h, &mut cr);
        crate::layout_plan::sidebar_shift(crate::layout_plan::content_bottom(cr.bottom)) != 0
    };
    show(slot(h, "simplehint"), sh.hint && !sidebar_short);
    show(slot(h, "details"), sh.details);
    show(slot(h, "audio"), sh.details);
    for id in [7, 12, 73] {
        show(GetDlgItem(h, id), sh.sources);
    }
}

pub(crate) unsafe fn hide_home(h: HWND) {
    for id in HOME_IDS {
        show(GetDlgItem(h, id), false);
    }
    for k in HOME_SLOTS {
        show(slot(h, k), false);
    }
}

/// Keep capture/audio failure text visible without covering the reset warning.
pub unsafe fn attention(h: HWND) {
    if prop(h, "display-settings") != 0 {
        return;
    }
    // Sidebar Clips page: the capture status rows stay hidden, even from the timer.
    let paged = crate::polished::sidebar()
        || crate::nav::compact_page(prop(h, "display-layout"), crate::polished::enabled(), false).is_some();
    if !crate::nav::home_text_allowed(paged, crate::polished::nav_page()) {
        return;
    }
    let warning = slot(h, "warning");
    SendMessageW(
        warning,
        WM_SETFONT,
        style::font(&crate::display::font_name(), 12, 400) as usize,
        0,
    );
    let encoder = slot(h, "encoder");
    let has_warning = !text(warning).is_empty();
    let mut r = RECT::default();
    GetWindowRect(encoder, &mut r);
    let mut p = POINT {
        x: r.left,
        y: r.top,
    };
    ScreenToClient(h, &mut p);
    put(warning, p.x, p.y - 2, r.right - r.left, 30);
    show(warning, has_warning);
    let details = prop(h, "compact-expanded") != 0;
    show(encoder, !has_warning && !details);
}
#[cfg(test)]
mod box_tests {
    use super::*;
    #[test]
    fn unpaged_legacy_layouts_keep_their_boxes() {
        for mode in [0usize, 2, 3, 4] {
            assert_eq!(boxes_for(mode, false, 700), boxes(mode, false), "{mode}");
        }
        assert!(boxes_for(1, true, 700).is_empty());
    }
}
