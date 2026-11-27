use std::path::Path;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::*,
        Input::KeyboardAndMouse::{EnableWindow, SetFocus},
        WindowsAndMessaging::*,
    },
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn text(h: HWND) -> String {
    let mut b = vec![0; (GetWindowTextLengthW(h) + 1).max(1) as usize];
    let n = GetWindowTextW(h, b.as_mut_ptr(), b.len() as i32);
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
unsafe fn control(
    p: HWND,
    f: HFONT,
    c: &str,
    t: &str,
    id: usize,
    x: i32,
    y: i32,
    w: i32,
    z: i32,
    style: u32,
) -> HWND {
    let h = CreateWindowExW(
        0,
        wide(c).as_ptr(),
        wide(t).as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
        x,
        y,
        w,
        z,
        p,
        id as _,
        GetModuleHandleW(std::ptr::null()),
        std::ptr::null(),
    );
    SendMessageW(h, WM_SETFONT, f as usize, 0);
    if c == "BUTTON" {
        crate::ui::interactive(h)
    }
    if c == "STATIC" {
        crate::ui::stable_label(h);
    }
    h
}
unsafe fn combo(p: HWND, f: HFONT, id: usize, x: i32, y: i32, w: i32, values: &[&str]) -> HWND {
    let c = control(
        p,
        f,
        "COMBOBOX",
        "",
        id,
        x,
        y,
        w,
        260,
        CBS_DROPDOWNLIST as u32 | CBS_OWNERDRAWFIXED as u32 | CBS_HASSTRINGS as u32 | WS_VSCROLL,
    );
    for s in values {
        SendMessageW(c, CB_ADDSTRING, 0, wide(s).as_ptr() as isize);
    }
    SendMessageW(c, CB_SETCURSEL, 0, 0);
    crate::ui::dropdown(c);
    c
}
pub unsafe fn toolbar(p: HWND, f: HFONT) -> HWND {
    let s = control(
        p,
        f,
        "EDIT",
        "",
        76,
        630,
        116,
        274,
        32,
        ES_AUTOHSCROLL as u32,
    );
    SendMessageW(
        s,
        EM_SETCUEBANNER as u32,
        0,
        wide("Search clips or tags...").as_ptr() as isize,
    );
    combo(
        p,
        f,
        77,
        630,
        154,
        132,
        &["Newest", "Oldest", "Longest", "Shortest"],
    );
    combo(p, f, 78, 770, 154, 134, &["All tags"]);
    crate::ui::search_field(s);
    s
}
pub unsafe fn refresh_tags(p: HWND, d: &Path) {
    let c = GetDlgItem(p, 78);
    if c.is_null() {
        return;
    }
    let tags = crate::catalog::reusable(d).unwrap_or_default();
    let (selected, favs) = {
        let f = crate::catalog::filter().lock().unwrap();
        (f.tag.clone(), f.favourites)
    };
    let signature = tags
        .keys()
        .fold(0usize, |n, s| {
            s.bytes()
                .fold(n, |n, b| n.wrapping_mul(31).wrapping_add(b as usize))
        })
        .wrapping_add(tags.len() + 1);
    let signature = signature.wrapping_add(if favs { 7919 } else { 0 });
    let signature = selected.bytes().fold(signature, |n, b| {
        n.wrapping_mul(31).wrapping_add(b as usize)
    });
    if GetPropW(c, wide("tag-signature").as_ptr()) as usize == signature {
        return;
    }
    SendMessageW(c, CB_RESETCONTENT, 0, 0);
    SendMessageW(c, CB_ADDSTRING, 0, wide("All tags").as_ptr() as isize);
    // Entry 1 is the favourites filter; tags follow.
    SendMessageW(c, CB_ADDSTRING, 0, wide(FAVOURITES_ENTRY).as_ptr() as isize);
    let mut index = if favs { 1 } else { 0 };
    for (i, t) in tags.keys().enumerate() {
        SendMessageW(c, CB_ADDSTRING, 0, wide(t).as_ptr() as isize);
        if *t == selected && !favs {
            index = i + 2
        }
    }
    if !selected.is_empty() && index == 0 {
        // Keep an active filter visible even when a folder has no matching tag.
        SendMessageW(c, CB_ADDSTRING, 0, wide(&selected).as_ptr() as isize);
        index = tags.len() + 2;
    }
    SendMessageW(c, CB_SETCURSEL, index, 0);
    SetPropW(c, wide("tag-signature").as_ptr(), signature as _);
}
/// Second entry of the tag filter box: show only favourite clips (plain text, no font-dependent glyph).
const FAVOURITES_ENTRY: &str = "Favourites";
pub unsafe fn read_toolbar(p: HWND, r: &Path) -> Result<(), String> {
    let query = text(GetDlgItem(p, 76));
    let sort = SendMessageW(GetDlgItem(p, 77), CB_GETCURSEL, 0, 0).clamp(0, 3) as usize;
    let c = GetDlgItem(p, 78);
    let pick = SendMessageW(c, CB_GETCURSEL, 0, 0);
    let favourites = pick == 1;
    let tag = if pick <= 1 { String::new() } else { text(c) };
    *crate::catalog::filter().lock().unwrap() = crate::catalog::Filter {
        query,
        oldest: sort == 1,
        sort,
        tag,
        favourites,
    };
    std::fs::write(r.join("library-sort.txt"), sort.to_string())
        .map_err(|e| format!("Sort applied, but not saved: {e}"))
}
/// Rounded tag chip: tinted fill, colour dot, name. Shared by the list and the live preview.
unsafe fn tag_chip(dc: HDC, r: RECT, name: &str, color: u32, selected: bool, muted: bool) {
    let t = crate::ui::style_theme();
    let fill = crate::ui::style::mix(t.panel, color, if selected { 26 } else { 16 });
    let edge = if selected { color } else { crate::ui::style::mix(t.panel, color, 34) };
    crate::ui::style::rect(dc, r, fill, edge, (r.bottom - r.top) / 2);
    let mid = (r.top + r.bottom) / 2;
    crate::ui::style::disc(dc, RECT { left: r.left + 12, top: mid - 5, right: r.left + 22, bottom: mid + 5 }, color, None);
    crate::ui::style::label(
        dc,
        name,
        RECT { left: r.left + 30, top: r.top, right: r.right - 12, bottom: r.bottom },
        crate::ui::style::font(&crate::display::font_name(), 14, 600),
        if muted { t.muted } else { t.text },
        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
    );
}
fn chip_width(name: &str, max: i32) -> i32 {
    (name.chars().count() as i32 * 9 + 46).clamp(70, max)
}
const PREVIEW_AREA: RECT = RECT { left: 350, top: 182, right: 780, bottom: 220 };
struct Manager {
    tags: Vec<(String, u32)>,
    selected: Option<usize>,
    color: usize,
    dirty: bool,
}
unsafe fn state(h: HWND) -> &'static mut Manager {
    &mut *(GetWindowLongPtrW(h, GWLP_USERDATA) as *mut Manager)
}
unsafe fn choose(h: HWND) {
    let i = SendMessageW(GetDlgItem(h, 403), LB_GETCURSEL, 0, 0);
    let s = state(h);
    if i >= 0 {
        if let Some((name, color)) = s.tags.get(i as usize) {
            s.selected = Some(i as usize);
            s.color = crate::catalog::palette()
                .iter()
                .position(|c| c == color)
                .unwrap_or(0);
            SetWindowTextW(GetDlgItem(h, 400), wide(name).as_ptr());
            EnableWindow(GetDlgItem(h, 406), 1);
        }
    }
    InvalidateRect(h, std::ptr::null(), 0);
    for i in 410..=417 {
        InvalidateRect(GetDlgItem(h, i), std::ptr::null(), 0);
    }
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_COMMAND => {
            let id = w & 0xffff;
            match id {
                403 if w >> 16 == LBN_SELCHANGE as usize => choose(h),
                400 if w >> 16 == EN_CHANGE as usize => {
                    let r = PREVIEW_AREA;
                    InvalidateRect(h, &r, 0);
                }
                405 => {
                    state(h).selected = None;
                    state(h).color = 0;
                    SetWindowTextW(GetDlgItem(h, 400), wide("").as_ptr());
                    EnableWindow(GetDlgItem(h, 406), 0);
                    SetFocus(GetDlgItem(h, 400));
                    for i in 410..=417 {
                        InvalidateRect(GetDlgItem(h, i), std::ptr::null(), 0);
                    }
                }
                401 => {
                    SetPropW(h, wide("action").as_ptr(), 1usize as _);
                }
                402 => {
                    SetPropW(h, wide("action").as_ptr(), 2usize as _);
                }
                406 => {
                    SetPropW(h, wide("action").as_ptr(), 3usize as _);
                }
                410..=417 => {
                    state(h).color = id - 410;
                    let r = PREVIEW_AREA;
                    InvalidateRect(h, &r, 0);
                    for i in 410..=417 {
                        InvalidateRect(GetDlgItem(h, i), std::ptr::null(), 0);
                    }
                }
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            SetPropW(h, wide("action").as_ptr(), 2usize as _);
            0
        }
        WM_MEASUREITEM => {
            let d = &mut *(l as *mut MEASUREITEMSTRUCT);
            if d.CtlType == ODT_LISTBOX {
                d.itemHeight = 44;
                1
            } else {
                crate::ui::handle(h, m, w, l).unwrap_or(0)
            }
        }
        WM_DRAWITEM => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            if d.CtlID == 403 {
                let s = state(h);
                let b = CreateSolidBrush(crate::ui::panel());
                FillRect(d.hDC, &d.rcItem, b);
                DeleteObject(b);
                if let Some((name, color)) = s.tags.get(d.itemID as usize) {
                    let r = d.rcItem;
                    let sel = d.itemState & ODS_SELECTED != 0;
                    let row = RECT { left: r.left + 4, top: r.top + 3, right: r.right - 4, bottom: r.bottom - 3 };
                    if sel {
                        crate::ui::style::rect(d.hDC, row, crate::ui::style::mix(crate::ui::panel(), crate::ui::border(), 60), crate::ui::border(), 12);
                    }
                    let c = RECT {
                        left: row.left + 8,
                        top: row.top + 5,
                        right: (row.left + 8 + chip_width(name, row.right - row.left - 16)).min(row.right - 8),
                        bottom: row.bottom - 5,
                    };
                    tag_chip(d.hDC, c, name, *color, sel, false);
                }
                1
            } else if (410..=417).contains(&d.CtlID) {
                let color = crate::catalog::palette()[(d.CtlID - 410) as usize];
                let b = CreateSolidBrush(crate::ui::panel());
                FillRect(d.hDC, &d.rcItem, b);
                DeleteObject(b);
                if state(h).color == (d.CtlID - 410) as usize {
                    crate::ui::style::disc(d.hDC, RECT { left: 4, top: 4, right: 36, bottom: 36 }, crate::ui::text_color(), None);
                    crate::ui::style::disc(d.hDC, RECT { left: 7, top: 7, right: 33, bottom: 33 }, color, None);
                } else {
                    crate::ui::style::disc(d.hDC, RECT { left: 5, top: 5, right: 35, bottom: 35 }, color, None);
                }
                1
            } else {
                crate::ui::handle(h, m, w, l).unwrap_or(0)
            }
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            SetTextColor(w as _, crate::ui::text_color());
            SetBkColor(w as _, crate::ui::panel());
            static B: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
            *B.get_or_init(|| CreateSolidBrush(crate::ui::panel()) as usize) as isize
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            let br = CreateSolidBrush(crate::ui::bg());
            FillRect(dc, &r, br);
            DeleteObject(br);
            crate::ui::style::rect(dc, RECT { left: 16, top: 92, right: 316, bottom: 424 }, crate::ui::panel(), crate::ui::border(), 10);
            crate::ui::style::rect(dc, RECT { left: 336, top: 92, right: 800, bottom: 424 }, crate::ui::panel(), crate::ui::border(), 10);
            let s = state(h);
            let t = crate::ui::style_theme();
            if s.tags.is_empty() {
                // Empty state, drawn in the left panel (the list is hidden until a tag exists).
                crate::ui::style::disc(dc, RECT { left: 138, top: 150, right: 194, bottom: 206 }, crate::ui::style::mix(t.panel, t.accent, 22), None);
                crate::ui::style::label(
                    dc,
                    "#",
                    RECT { left: 138, top: 150, right: 194, bottom: 206 },
                    crate::ui::style::font(&crate::display::font_name(), 28, 700),
                    t.accent,
                    DT_SINGLELINE | DT_CENTER | DT_VCENTER,
                );
                crate::ui::style::label(
                    dc,
                    "No tags yet",
                    RECT { left: 24, top: 218, right: 308, bottom: 246 },
                    crate::ui::style::font(&crate::display::font_name(), 17, 700),
                    t.text,
                    DT_SINGLELINE | DT_CENTER | DT_VCENTER,
                );
                crate::ui::style::label(
                    dc,
                    "Tags are labels for finding clips later. Type a name on the right, pick a colour, then press Save tag.",
                    RECT { left: 36, top: 252, right: 296, bottom: 330 },
                    crate::ui::style::font(&crate::display::font_name(), 13, 400),
                    t.muted,
                    DT_CENTER | DT_WORDBREAK,
                );
            }
            // Live preview of the tag being typed or edited.
            let typed = text(GetDlgItem(h, 400));
            let typed = typed.trim();
            let color = crate::catalog::palette()[s.color.min(crate::catalog::palette().len() - 1)];
            crate::ui::style::label(
                dc,
                "Preview",
                RECT { left: 350, top: PREVIEW_AREA.top, right: 420, bottom: PREVIEW_AREA.bottom },
                crate::ui::style::font(&crate::display::font_name(), 13, 400),
                t.muted,
                DT_SINGLELINE | DT_VCENTER,
            );
            let (shown, muted) = if typed.is_empty() { ("Tag name", true) } else { (typed, false) };
            tag_chip(
                dc,
                RECT { left: 424, top: PREVIEW_AREA.top + 3, right: 424 + chip_width(shown, 340), bottom: PREVIEW_AREA.bottom - 3 },
                shown,
                color,
                false,
                muted,
            );
            crate::ui::end_buffer(h, &ps, dc, b);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
pub unsafe fn manage_tags(parent: HWND, font: HFONT, dir: &Path) -> Result<(), String> {
    let tags = crate::catalog::reusable(dir)?.into_iter().collect();
    let mut data = Box::new(Manager {
        tags,
        selected: None,
        color: 0,
        dirty: false,
    });
    let cls = wide("ZimClipTagManager");
    let inst = GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let mut r = RECT::default();
    GetWindowRect(parent, &mut r);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..std::mem::zeroed()
    };
    GetMonitorInfoW(
        MonitorFromWindow(parent, MONITOR_DEFAULTTONEAREST),
        &mut info,
    );
    let x = (r.left + ((r.right - r.left) - 820) / 2).clamp(
        info.rcWork.left,
        (info.rcWork.right - 820).max(info.rcWork.left),
    );
    let y = (r.top + ((r.bottom - r.top) - 500) / 2).clamp(
        info.rcWork.top,
        (info.rcWork.bottom - 500).max(info.rcWork.top),
    );
    let h = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_LAYERED,
        cls.as_ptr(),
        wide("Manage tags").as_ptr(),
        WS_POPUP | WS_CLIPCHILDREN,
        x,
        y,
        820,
        500,
        parent,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    SetWindowLongPtrW(h, GWLP_USERDATA, &mut *data as *mut _ as isize);
    crate::ui::clip_round(h, 24);
    for (t, x, y, w, z) in [
        ("Manage tags", 24, 18, 760, 30),
        (
            "Create reusable labels for finding clips later.",
            24,
            52,
            760,
            26,
        ),
        ("Your tags", 24, 104, 280, 28),
        ("Tag name", 350, 102, 430, 24),
        ("Colour", 350, 226, 430, 24),
        (
            "Choose a tag to edit, or create a new one.",
            350,
            352,
            430,
            22,
        ),
    ] {
        control(h, font, "STATIC", t, 0, x, y, w, z, 0);
    }
    let list = control(
        h,
        font,
        "LISTBOX",
        "",
        403,
        24,
        140,
        280,
        220,
        LBS_NOTIFY as u32
            | LBS_OWNERDRAWFIXED as u32
            | LBS_HASSTRINGS as u32
            | WS_VSCROLL,
    );
    for (n, _) in &data.tags {
        SendMessageW(list, LB_ADDSTRING, 0, wide(n).as_ptr() as isize);
    }
    if data.tags.is_empty() {
        ShowWindow(list, SW_HIDE);
    }
    let name_edit = control(
        h,
        font,
        "EDIT",
        "",
        400,
        350,
        130,
        430,
        42,
        ES_AUTOHSCROLL as u32 | WS_BORDER,
    );
    crate::ui::search_field(name_edit);
    for i in 0..8 {
        control(
            h,
            font,
            "BUTTON",
            "",
            410 + i,
            350 + (i % 5) as i32 * 78,
            254 + (i / 5) as i32 * 48,
            42,
            42,
            BS_OWNERDRAW as u32,
        );
    }
    control(
        h,
        font,
        "BUTTON",
        "Create new tag",
        405,
        24,
        376,
        280,
        40,
        BS_OWNERDRAW as u32,
    );
    control(
        h,
        font,
        "BUTTON",
        "Save tag",
        401,
        350,
        380,
        210,
        40,
        BS_OWNERDRAW as u32,
    );
    control(
        h,
        font,
        "BUTTON",
        "Delete tag",
        406,
        570,
        380,
        210,
        40,
        BS_OWNERDRAW as u32,
    );
    EnableWindow(GetDlgItem(h, 406), 0);
    control(
        h,
        font,
        "BUTTON",
        "Done",
        402,
        610,
        444,
        170,
        38,
        BS_OWNERDRAW as u32,
    );
    EnableWindow(parent, 0);
    ShowWindow(h, SW_SHOW);
    SetForegroundWindow(h);
    SetFocus(GetDlgItem(h, 400));
    let mut msg = MSG::default();
    let animate = crate::ui::motion::allowed(h);
    let opened = std::time::Instant::now();
    let mut settled = !animate;
    SetLayeredWindowAttributes(h, 0, if animate { 0 } else { 255 }, LWA_ALPHA);
    loop {
        if !settled {
            // Same pop-in as the other dialogs: about 150 ms fade with a 6 px rise, ease-out.
            let t = (opened.elapsed().as_millis() as f32 / 150.0).min(1.0);
            let e = 1.0 - (1.0 - t) * (1.0 - t);
            SetLayeredWindowAttributes(h, 0, (e * 255.0) as u8, LWA_ALPHA);
            SetWindowPos(h, std::ptr::null_mut(), x, y + ((1.0 - e) * 6.0) as i32, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            settled = t >= 1.0;
        }
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                PostQuitMessage(msg.wParam as i32);
                SetPropW(h, wide("action").as_ptr(), 2usize as _);
                break;
            }
            if msg.message == WM_KEYDOWN && msg.wParam == 27 {
                SetPropW(h, wide("action").as_ptr(), 2usize as _);
            } else if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let a = GetPropW(h, wide("action").as_ptr()) as usize;
        if a == 2 {
            break;
        }
        if a == 3 {
            SetPropW(h, wide("action").as_ptr(), std::ptr::null_mut());
            if let Some(i) = data.selected {
                let name = data.tags[i].0.clone();
                let used = crate::catalog::tag_use_count(dir, &name);
                let body = format!(
                    "Delete tag?\nThe tag \"{name}\" will be removed from {used} clip{}. The videos are not changed.",
                    if used == 1 { "" } else { "s" }
                );
                if crate::editor::confirm_themed(h, font, &body, "Delete tag") {
                    match crate::catalog::delete_palette_tag(dir, &name) {
                        Ok(()) => match crate::catalog::reusable(dir) {
                            Ok(t) => {
                                data.tags = t.into_iter().collect();
                                data.dirty = true;
                                data.selected = None;
                                data.color = 0;
                                SetWindowTextW(GetDlgItem(h, 400), wide("").as_ptr());
                                EnableWindow(GetDlgItem(h, 406), 0);
                                SendMessageW(list, LB_RESETCONTENT, 0, 0);
                                for (n, _) in &data.tags {
                                    SendMessageW(list, LB_ADDSTRING, 0, wide(n).as_ptr() as isize);
                                }
                                if data.tags.is_empty() {
                                    ShowWindow(list, SW_HIDE);
                                }
                                InvalidateRect(h, std::ptr::null(), 0);
                            }
                            Err(e) => {
                                MessageBoxW(h, wide(&e).as_ptr(), wide("Could not reload tags").as_ptr(), MB_OK);
                            }
                        },
                        Err(e) => {
                            MessageBoxW(h, wide(&e).as_ptr(), wide("Tag not deleted").as_ptr(), MB_OK);
                        }
                    }
                }
            }
        }
        if a == 1 {
            SetPropW(h, wide("action").as_ptr(), std::ptr::null_mut());
            let name = text(GetDlgItem(h, 400));
            let name = name.trim();
            let valid = crate::catalog::normalize(name).is_ok_and(|n| n == name)
                && !name.is_empty()
                && !name.contains(',');
            if !valid {
                MessageBoxW(
                    h,
                    wide("Use one tag name, up to 24 characters.").as_ptr(),
                    wide("Tag not saved").as_ptr(),
                    MB_OK,
                );
                continue;
            }
            let result = if let Some(i) = data.selected {
                crate::catalog::rename_palette_tag(
                    dir,
                    &data.tags[i].0,
                    name,
                    crate::catalog::palette()[data.color],
                )
            } else {
                crate::catalog::save_color(dir, name, crate::catalog::palette()[data.color])
            };
            match result {
                Ok(()) => {
                    data.tags = match crate::catalog::reusable(dir) {
                        Ok(t) => t.into_iter().collect(),
                        Err(e) => {
                            MessageBoxW(
                                h,
                                wide(&e).as_ptr(),
                                wide("Could not reload tags").as_ptr(),
                                MB_OK,
                            );
                            continue;
                        }
                    };
                    data.dirty = true;
                    ShowWindow(list, SW_SHOW);
                    SendMessageW(list, LB_RESETCONTENT, 0, 0);
                    for (n, _) in &data.tags {
                        SendMessageW(list, LB_ADDSTRING, 0, wide(n).as_ptr() as isize);
                    }
                    data.selected = data.tags.iter().position(|t| t.0 == name);
                    if let Some(i) = data.selected {
                        SendMessageW(list, LB_SETCURSEL, i, 0);
                    }
                    InvalidateRect(h, std::ptr::null(), 0);
                }
                Err(e) => {
                    MessageBoxW(h, wide(&e).as_ptr(), wide("Tag not saved").as_ptr(), MB_OK);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    DestroyWindow(h);
    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RedrawWindow(
        parent,
        std::ptr::null(),
        std::ptr::null_mut(),
        RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
    );
    Ok(())
}

// Replay library migration. Original IDs and message handlers remain the action boundary.
/// Rows the library can draw and count right now, from the window's own state. Every consumer
/// (drawing, offset limits, paging, wheel) uses this one value.
pub unsafe fn capacity_for(h: HWND) -> usize {
    let mut cr = RECT::default();
    GetClientRect(h, &mut cr);
    let mode = GetPropW(h, wide("display-layout").as_ptr()) as usize;
    let large = GetPropW(h, wide("library-large").as_ptr()) as usize != 0;
    let polished = crate::polished::enabled();
    // Legacy Stacked paging depends on the client height; derive it here, not from the last layout.
    crate::nav::set_stacked_short(cr.bottom < crate::layout_plan::LEGACY_STACKED_SIDE_BY_SIDE_MIN_H);
    let cpage1 = crate::nav::compact_page(mode, polished, false) == Some(1);
    let gmode = if cpage1 { if mode == 3 { 6 } else { 5 } } else { mode };
    crate::layout_plan::rows_fit(&crate::layout_plan::lib_geom(gmode, mode, large, polished, cpage1, cr.right, cr.bottom))
}
unsafe fn move_to(h: HWND, x: i32, y: i32, w: i32, z: i32) {
    crate::display::put(h, x, y, w, z);
}
pub unsafe fn layout(h: HWND, mode: usize, rows: &[HWND], large: bool) {
    SetPropW(h, wide("library-large").as_ptr(), large as usize as _);
    // Settings owns the window: never re-show library controls over it.
    if !GetPropW(h, wide("display-settings").as_ptr()).is_null() {
        return;
    }
    let mut cr = RECT::default();
    GetClientRect(h, &mut cr);
    let slot = |key: &str| GetPropW(h, wide(&format!("slot-{key}")).as_ptr()) as HWND;
    let count = GetPropW(h, wide("library-count").as_ptr()) as usize;
    let offset = GetPropW(h, wide("library-offset").as_ptr()) as usize;
    let polished = crate::polished::enabled();
    // Compact has two pages (Home, Clips). The Clips page uses the roomy geometry.
    let cpage = crate::nav::compact_page(mode, polished, false);
    let gmode = if cpage == Some(1) { if mode == 3 { 6 } else { 5 } } else { mode };
    let g = crate::layout_plan::lib_geom(gmode, mode, large, polished, cpage == Some(1), cr.right, cr.bottom);
    let (x, y, w, end) = (g.x, g.y, g.w, g.end);
    for id in [36, 90, 37, 76, 77, 78, 70, 81, 85] {
        ShowWindow(GetDlgItem(h, id), SW_SHOW);
    }
    SetPropW(
        GetDlgItem(h, 90),
        wide("density-selected").as_ptr(),
        large as usize as _,
    );
    SetPropW(
        GetDlgItem(h, 36),
        wide("density-selected").as_ptr(),
        (!large) as usize as _,
    );
    ShowWindow(slot("rightheading"), SW_SHOW);
    ShowWindow(slot("righthint"), SW_SHOW);
    move_to(slot("rightheading"), x, y, 200, 20);
    let density_h = if gmode == 1 { 28 } else { 32 };
    // Polished: a pair of 36 px icon buttons (Large, Compact). Legacy keeps the text button.
    let pair_w = if polished { 36 } else { 100 };
    // The count text stops left of the density pair, so it never covers the buttons' lower edge.
    move_to(slot("righthint"), x, y + 24, w - (2 * pair_w + 16).max(116), 22);
    move_to(GetDlgItem(h, 90), x + w - 2 * pair_w, y, pair_w, density_h);
    move_to(GetDlgItem(h, 36), x + w - pair_w, y, pair_w, density_h);
    let compact = gmode == 1;
    let stacked = mode == 3;
    let filter_y = if compact { y + 40 } else { y + 52 };
    let search_w = if compact {
        232
    } else if stacked {
        440
    } else {
        w
    };
    move_to(GetDlgItem(h, 76), x, filter_y, search_w, 32);
    let (cx, cy, cw) = if compact {
        (x + 240, filter_y, 140)
    } else if stacked {
        (x + 452, filter_y, 216)
    } else {
        (x, filter_y + if large { 40 } else { 44 }, (w - 12) / 2)
    };
    move_to(GetDlgItem(h, 77), cx, cy, cw, 240);
    move_to(
        GetDlgItem(h, 78),
        cx + cw + 8,
        cy,
        if compact { 148 } else { cw },
        240,
    );
    if compact {
        ShowWindow(slot("righthint"), SW_HIDE);
    }
    let top = g.top;
    let selecting = crate::library::state().lock().unwrap().active;
    let selected = crate::library::state().lock().unwrap().paths.len();
    let cap = crate::layout_plan::rows_fit(&g);
    // A shorter client shows fewer rows: keep the paged-in offset inside what can be reached.
    let offset = offset.min(count.saturating_sub(cap));
    SetPropW(h, wide("library-offset").as_ptr(), offset as _);
    // A row is shown only when the list was filled for it (refresh_recent records how many rows it
    // filled), so a capacity change can never reveal an empty row before the next refresh.
    let filled_limit = match GetPropW(h, wide("library-filled").as_ptr()) as usize { 0 => usize::MAX, n => n - 1 };
    for (i, row) in rows.iter().enumerate() {
        let (rx, ry, rw, rh) = crate::layout_plan::row_rect(&g, i);
        SetPropW(*row, wide("library-grid").as_ptr(), g.grid as usize as _);
        SetPropW(
            *row,
            wide("library-dense").as_ptr(),
            ((compact || stacked) && !g.grid) as usize as _,
        );
        move_to(*row, rx, ry, rw, rh);
        ShowWindow(
            *row,
            if i < count.saturating_sub(offset).min(cap).min(filled_limit) {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
    }
    move_to(
        GetDlgItem(h, 37),
        x + w - 6,
        top,
        6,
        (end - 42 - top).max(40),
    );
    let ay = end - crate::layout_plan::ACTION_H;
    let primary = if selecting {
        GetDlgItem(h, 71)
    } else {
        slot("open")
    };
    ShowWindow(slot("open"), if selecting { SW_HIDE } else { SW_SHOW });
    ShowWindow(GetDlgItem(h, 71), if selecting { SW_SHOW } else { SW_HIDE });
    if selecting {
        SetWindowTextW(
            primary,
            wide(&format!("Delete selected ({selected})")).as_ptr(),
        );
    }
    let actionw = (w - 308).max(80);
    move_to(primary, x, ay, actionw, 32);
    move_to(GetDlgItem(h, 70), x + w - 300, ay, 88, 32);
    move_to(GetDlgItem(h, 81), x + w - 204, ay, 100, 32);
    move_to(GetDlgItem(h, 85), x + w - 96, ay, 80, 32);
    SetWindowTextW(
        GetDlgItem(h, 70),
        wide(if selecting { "Done" } else { "Select" }).as_ptr(),
    );
    // Empty/no-match state uses the existing honest count text, without fabricated skeletons.
    if count == 0 {
        move_to(
            slot("righthint"),
            x + 8,
            top + 16,
            w - 24,
            (ay - top - 20).max(20),
        );
        ShowWindow(slot("righthint"), SW_SHOW);
    }
    if compact {
        // Compact Home: capture controls only, with room between rows (the library has its own page).
        let put = |id, x, y, w, z| move_to(GetDlgItem(h, id), x, y, w, z);
        let expanded = GetPropW(h, wide("compact-expanded").as_ptr()) as usize != 0;
        ShowWindow(slot("simplehint"), SW_HIDE);
        move_to(slot("encoder"), 34, 136, 536, 24);
        move_to(slot("buffer"), 34, 170, 536, 44);
        // Rows are measured top to bottom: Save, heading, two sources, voice, note, buttons.
        // The result row, buttons and footer are anchored to the client bottom (about 88 px), so
        // on a short work area the whole stack is tightened to end above them instead of overlapping.
        let short = cr.bottom < 576 && !expanded;
        if short {
            move_to(slot("buffer"), 34, 166, 536, 40);
            put(1, 34, 210, 536, 44);
            ShowWindow(slot("sources"), SW_HIDE);
            put(7, 34, 258, 260, 42);
            put(12, 310, 258, 260, 42);
            put(73, 34, 304, 536, 36);
            move_to(slot("source-note"), 34, 342, 536, 28);
        } else {
            put(1, 34, 222, 536, 50);
            move_to(slot("sources"), 34, 282, 536, 18);
            // Coming back from a short window: the heading was hidden there, bring it back.
            if !expanded {
                ShowWindow(slot("sources"), SW_SHOW);
            }
            put(7, 34, 304, 260, 48);
            put(12, 310, 304, 260, 48);
            put(73, 34, 360, 536, 46);
            move_to(slot("source-note"), 34, 410, 536, 28);
        }
        // Expanded: the details text starts below the status line (136 + 24) so no line is covered.
        // Recover sits inside the card: 14 px below the button row (446-486) and 12 px above the bottom edge (540).
        put(44, 34, 500, 220, 28);
        let by = if expanded { 400 } else if short { 374 } else { 446 };
        let bh = if short { 36 } else { 40 };
        put(10, 34, by, 170, bh);
        put(43, 214, by, 170, bh);
        put(40, 394, by, 176, bh);
        if expanded {
            // Expanded keeps Save available: details text, then Save, then the audio text.
            put(1, 34, 236, 536, 50);
            move_to(slot("details"), 34, 166, 536, 64);
            move_to(slot("audio"), 34, 294, 536, 98);
        }
    }
    SetPropW(
        slot("result"),
        wide("footer-background").as_ptr(),
        1usize as _,
    );
    SetPropW(
        slot("footer"),
        wide("footer-background").as_ptr(),
        1usize as _,
    );
    SetPropW(
        slot("result"),
        wide("status-single-line").as_ptr(),
        1usize as _,
    );
    // One intentional ellipsized status row. Native text retains the full message
    // for accessibility; the old 24px row accidentally clipped wrapped lines.
    // Align status and diagnostics on the background, outside card boundaries.
    let sx = if mode == 4 { crate::polished::SB } else { 0 };
    move_to(
        slot("result"),
        24 + sx,
        cr.bottom - crate::layout_plan::RESULT_Y,
        (cr.right - 48 - sx).max(100),
        24,
    );
    move_to(GetDlgItem(h, 44), cr.right - 470, cr.bottom - crate::layout_plan::BUTTON_Y, 160, 28);
    move_to(GetDlgItem(h, 4), cr.right - 300, cr.bottom - crate::layout_plan::BUTTON_Y, 120, 28);
    move_to(GetDlgItem(h, 5), cr.right - 168, cr.bottom - crate::layout_plan::BUTTON_Y, 144, 28);
    move_to(slot("footer"), 24 + sx, cr.bottom - crate::layout_plan::FOOTER_Y, cr.right - 48 - sx, 22);
    if mode == 4 {
        crate::replay_design::sidebar_hide_library(h);
    }
    if let Some(page) = cpage {
        compact_pages(h, mode, page, rows);
    }
}
/// Compact pages: exactly one of the capture controls or the library controls is visible.
unsafe fn compact_pages(h: HWND, mode: usize, page: usize, rows: &[HWND]) {
    let slot = |key: &str| GetPropW(h, wide(&format!("slot-{key}")).as_ptr()) as HWND;
    let hide_or_show = |ids: &[i32], slots: &[&str], on: bool| {
        for id in ids {
            let c = GetDlgItem(h, *id);
            if !c.is_null() {
                ShowWindow(c, if on { SW_SHOW } else { SW_HIDE });
            }
        }
        for k in slots {
            let c = slot(k);
            if !c.is_null() {
                ShowWindow(c, if on { SW_SHOW } else { SW_HIDE });
            }
        }
    };
    if page == 0 {
        hide_or_show(&crate::replay_design::CLIPS_IDS, &crate::replay_design::CLIPS_SLOTS, false);
        // Coming back from Clips: the shared Home rules restore every control.
        crate::replay_design::show_home(h, false);
        crate::replay_design::attention(h);
        // The Compact hint has no row of its own; keep it hidden so it cannot cover the encoder line.
        if mode == 1 {
            ShowWindow(slot("simplehint"), SW_HIDE);
            // Short work area: the tightened Home stack has no row for the Sources heading.
            let mut cr = RECT::default();
            GetClientRect(h, &mut cr);
            if cr.bottom < 576 && GetPropW(h, wide("compact-expanded").as_ptr()) as usize == 0 {
                ShowWindow(slot("sources"), SW_HIDE);
            }
        } else {
            // Stacked places its own hint row but has no separate sources heading.
            ShowWindow(slot("sources"), SW_HIDE);
            ShowWindow(slot("privacy"), SW_HIDE);
        }
        for r in rows {
            ShowWindow(*r, SW_HIDE);
        }
        // The two page buttons stay visible on both pages.
        for id in [110, 111] {
            ShowWindow(GetDlgItem(h, id), SW_SHOW);
        }
    } else {
        crate::replay_design::hide_home(h);
        hide_or_show(&[86, 87], &["warning", "encoder"], false);
        for id in [110, 111] {
            ShowWindow(GetDlgItem(h, id), SW_SHOW);
        }
    }
}
pub unsafe fn draw_card(
    d: &DRAWITEMSTRUCT,
    path: Option<std::path::PathBuf>,
    bitmap: Option<usize>,
    tags: Vec<(String, u32)>,
) {
    use crate::ui::style::{self, State};
    let t = crate::ui::style_theme();
    let f = style::font(&crate::display::font_name(), 14, 600);
    let small = style::font(&crate::display::font_name(), 12, 400);
    let selected = path.as_ref().is_some_and(|p| crate::library::selected(p));
    let mut pt = POINT::default();
    GetCursorPos(&mut pt);
    ScreenToClient(d.hwndItem, &mut pt);
    let hover = pt.x >= 0 && pt.y >= 0 && pt.x < d.rcItem.right && pt.y < d.rcItem.bottom;
    let fill = if selected || hover { t.hover } else { t.raise };
    FillRect(d.hDC, &d.rcItem, style::brush(t.panel));
    style::rect(
        d.hDC,
        d.rcItem,
        fill,
        if selected { t.accent } else { t.border },
        10,
    );
    if selected || hover {
        FillRect(
            d.hDC,
            &RECT {
                left: 2,
                top: 8,
                right: 5,
                bottom: d.rcItem.bottom - 8,
            },
            style::brush(t.accent),
        );
    }
    let grid = GetPropW(d.hwndItem, wide("library-grid").as_ptr()) as usize != 0;
    let compact = GetPropW(d.hwndItem, wide("library-dense").as_ptr()) as usize != 0;
    let wide_card = grid && crate::layout_plan::is_wide_card(d.rcItem.right, d.rcItem.bottom);
    let thumb = if wide_card {
        RECT { left: 8, top: 8, right: 136, bottom: 80 }
    } else if grid {
        RECT {
            left: 8,
            top: 8,
            right: d.rcItem.right - 8,
            bottom: 8 + (d.rcItem.right - 16) * 9 / 16,
        }
    } else {
        let th = (d.rcItem.bottom - 12).min(44);
        RECT {
            left: 10,
            top: 6,
            right: 88,
            bottom: 6 + th,
        }
    };
    if !compact {
        style::rect(d.hDC, thumb, t.bg, t.border, 6);
        if let Some(bitmap) = bitmap {
            let dc = CreateCompatibleDC(d.hDC);
            let old = SelectObject(dc, bitmap as _);
            let mut info = BITMAP::default();
            GetObjectW(
                bitmap as _,
                std::mem::size_of::<BITMAP>() as _,
                &mut info as *mut _ as _,
            );
            SetStretchBltMode(d.hDC, HALFTONE);
            // Fill the frame without stretching: crop the picture to the frame's shape.
            let (dw, dh) = (thumb.right - thumb.left, thumb.bottom - thumb.top);
            let (sx, sy, sw, sh) = crate::layout_plan::cover_crop(info.bmWidth, info.bmHeight, dw, dh);
            StretchBlt(d.hDC, thumb.left, thumb.top, dw, dh, dc, sx, sy, sw, sh, SRCCOPY);
            SelectObject(dc, old);
            DeleteDC(dc);
        } else {
            style::label(
                d.hDC,
                "Play",
                thumb,
                small,
                t.muted,
                DT_SINGLELINE | DT_CENTER | DT_VCENTER,
            );
        }
    }
    let title = path
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let tx = if wide_card { 148 } else if grid || compact { 12 } else { 100 };
    let ty = if wide_card {
        12
    } else if grid {
        thumb.bottom + 6
    } else if compact {
        1
    } else {
        if d.rcItem.bottom < 60 {
            2
        } else {
            7
        }
    };
    style::label(
        d.hDC,
        &title,
        RECT {
            left: tx,
            top: ty,
            right: d.rcItem.right - if compact { 306 } else { 72 },
            bottom: if compact { d.rcItem.bottom } else { ty + 22 },
        },
        if compact { small } else { f },
        t.text,
        DT_SINGLELINE | DT_END_ELLIPSIS | DT_VCENTER | DT_NOPREFIX,
    );
    if compact {
        let caption = text(d.hwndItem);
        let meta = caption.split_once('\n').map(|(_, s)| s.replace('\n', " \u{b7} ")).unwrap_or_default();
        style::label(
            d.hDC,
            &meta,
            RECT {
                left: d.rcItem.right - 270,
                top: 0,
                right: d.rcItem.right - 60,
                bottom: d.rcItem.bottom,
            },
            small,
            t.muted,
            DT_SINGLELINE | DT_VCENTER | DT_RIGHT,
        );
    }
    if !compact {
        let caption = text(d.hwndItem);
        // Size and the file's modified date ("Date modified"), not a recording date.
        let meta = caption.split_once('\n').map(|(_, s)| s.replace('\n', " \u{b7} ")).unwrap_or_default();
        let y = ty + 24;
        let meta_w = if grid && !wide_card { d.rcItem.right - 76 - tx } else { 188 };
        style::label(
            d.hDC,
            &meta,
            RECT {
                left: tx,
                top: y,
                right: tx + meta_w,
                bottom: y + 18,
            },
            small,
            t.muted,
            DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        // Grid cards show the first tag over the picture; rows show tags after the details.
        let chip = grid && !wide_card;
        let mut x = if chip { thumb.left + 8 } else { tx + meta_w + 4 };
        let tag_y = if chip { thumb.top + 8 } else { y };
        let tag_limit = if chip { thumb.right - 8 } else { d.rcItem.right - 72 };
        for (tag, color) in tags {
            let width = (tag.chars().count() as i32 * 7 + 14).min(118);
            if x + width > tag_limit {
                break;
            }
            let r = RECT {
                left: x,
                top: tag_y,
                right: x + width,
                bottom: tag_y + 18,
            };
            let polished = crate::polished::enabled();
            let tag_fill = if polished {
                style::mix(t.panel, color, 13)
            } else {
                style::mix(fill, color, 15)
            };
            style::rect(d.hDC, r, tag_fill, tag_fill, 9);
            style::label(
                d.hDC,
                &tag,
                r,
                small,
                if polished { color } else { t.text },
                DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_END_ELLIPSIS,
            );
            x += width + 4;
            if chip {
                break;
            }
        }
    }
    if path.is_some() && !crate::library::state().lock().unwrap().active {
        draw_trash(d, grid, compact, &pt);
        if let Some(p) = path.as_ref() {
            draw_star(d, grid, compact, &pt, crate::favourites::is_favourite(p));
        }
    }
    let _ = State::default();
    if windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() == d.hwndItem {
        style::focus(d.hDC, d.rcItem, t);
    }
}

/// Hit rectangle of the favourite star on a library card, in card client coordinates.
pub unsafe fn star_hit(h: HWND) -> RECT {
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let grid = GetPropW(h, wide("library-grid").as_ptr()) as usize != 0;
    let dense = GetPropW(h, wide("library-dense").as_ptr()) as usize != 0;
    let (l, t, rr, b) = crate::layout_plan::star_rect(r.right, r.bottom, grid, dense);
    RECT { left: l, top: t, right: rr, bottom: b }
}

unsafe fn draw_star(d: &DRAWITEMSTRUCT, grid: bool, dense: bool, cursor: &POINT, on: bool) {
    use crate::ui::style;
    use windows_sys::Win32::Graphics::Gdi::{CreatePen, Polygon, PS_SOLID};
    let t = crate::ui::style_theme();
    let (l, tp, r, b) = crate::layout_plan::star_rect(d.rcItem.right, d.rcItem.bottom, grid, dense);
    let over = cursor.x >= l && cursor.x < r && cursor.y >= tp && cursor.y < b;
    if over {
        style::rect(d.hDC, RECT { left: l, top: tp, right: r, bottom: b }, t.hover, t.border, 8);
    }
    let colour = if on { t.accent } else if over { t.text } else { t.muted };
    let pen = CreatePen(PS_SOLID, 1, colour);
    let brush = if on { style::brush(t.accent) } else { windows_sys::Win32::Graphics::Gdi::GetStockObject(windows_sys::Win32::Graphics::Gdi::NULL_BRUSH) as _ };
    let (op, ob) = (SelectObject(d.hDC, pen as _), SelectObject(d.hDC, brush as _));
    let (cx, cy) = (((l + r) / 2) as f32, ((tp + b) / 2) as f32 + 1.0);
    let outer = if dense { 7.0f32 } else { 9.0 };
    let inner = outer * 0.42;
    let pts: Vec<POINT> = (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let rad = if i % 2 == 0 { outer } else { inner };
            POINT { x: (cx + rad * a.cos()).round() as i32, y: (cy + rad * a.sin()).round() as i32 }
        })
        .collect();
    Polygon(d.hDC, pts.as_ptr(), pts.len() as i32);
    SelectObject(d.hDC, op);
    SelectObject(d.hDC, ob);
    DeleteObject(pen as _);
}

/// Hit rectangle of the trash button on a library card, in card client coordinates.
pub unsafe fn trash_hit(h: HWND) -> RECT {
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let grid = GetPropW(h, wide("library-grid").as_ptr()) as usize != 0;
    let dense = GetPropW(h, wide("library-dense").as_ptr()) as usize != 0;
    let (l, t, rr, b) = crate::layout_plan::trash_rect(r.right, r.bottom, grid, dense);
    RECT { left: l, top: t, right: rr, bottom: b }
}

unsafe fn draw_trash(d: &DRAWITEMSTRUCT, grid: bool, dense: bool, cursor: &POINT) {
    use crate::ui::style;
    use windows_sys::Win32::Graphics::Gdi::{CreatePen, LineTo, MoveToEx, PS_SOLID};
    let t = crate::ui::style_theme();
    let (l, tp, r, b) = crate::layout_plan::trash_rect(d.rcItem.right, d.rcItem.bottom, grid, dense);
    let rect = RECT { left: l, top: tp, right: r, bottom: b };
    let over = cursor.x >= l && cursor.x < r && cursor.y >= tp && cursor.y < b;
    if over {
        style::rect(d.hDC, rect, t.hover, t.border, 8);
    }
    let pen = CreatePen(PS_SOLID, 1, if over { t.text } else { t.muted });
    let old = SelectObject(d.hDC, pen as _);
    let (cx, cy) = ((l + r) / 2, (tp + b) / 2);
    let u = if dense { 6 } else { 8 };
    let line = |x1: i32, y1: i32, x2: i32, y2: i32| {
        MoveToEx(d.hDC, x1, y1, std::ptr::null_mut());
        LineTo(d.hDC, x2, y2);
    };
    // Lid and handle.
    line(cx - u, cy - u / 2, cx + u + 1, cy - u / 2);
    line(cx - u / 3, cy - u / 2 - 2, cx + u / 3 + 1, cy - u / 2 - 2);
    line(cx - u / 3, cy - u / 2 - 2, cx - u / 3, cy - u / 2);
    line(cx + u / 3, cy - u / 2 - 2, cx + u / 3, cy - u / 2);
    // Bin body and two slats.
    line(cx - u + 2, cy - u / 2, cx - u + 3, cy + u);
    line(cx + u - 2, cy - u / 2, cx + u - 3, cy + u);
    line(cx - u + 3, cy + u, cx + u - 2, cy + u);
    line(cx - 1, cy - u / 2 + 3, cx - 1, cy + u - 3);
    line(cx + 2, cy - u / 2 + 3, cx + 2, cy + u - 3);
    SelectObject(d.hDC, old);
    DeleteObject(pen as _);
}
