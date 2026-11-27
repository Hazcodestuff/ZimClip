use std::{
    path::Path,
    sync::{OnceLock, RwLock},
};
fn face() -> &'static RwLock<String> {
    static F: OnceLock<RwLock<String>> = OnceLock::new();
    F.get_or_init(|| RwLock::new("Segoe UI Variable Text".into()))
}
pub fn font_name() -> String {
    face().read().unwrap().clone()
}
pub fn load(root: &Path) {
    // Respect an explicit family selection. Resolve the reference's native stack
    // only for a fresh/default configuration, avoiding GDI's legacy bitmap fallback.
    let saved = std::fs::read_to_string(root.join("font.txt")).unwrap_or_default();
    let s = saved.trim();
    if !s.is_empty()
        && s != "Segoe UI Variable Text"
        && s != "Segoe UI"
        && s.encode_utf16().count() < 32
        && !s.chars().any(char::is_control)
    {
        *face().write().unwrap() = s.into();
        return;
    }
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::*;
        let dc = GetDC(std::ptr::null_mut());
        let mut fonts = std::collections::BTreeSet::<String>::new();
        let lf = LOGFONTW {
            lfCharSet: DEFAULT_CHARSET,
            ..std::mem::zeroed()
        };
        EnumFontFamiliesExW(
            dc,
            &lf,
            Some(collect_font),
            &mut fonts as *mut _ as isize,
            0,
        );
        ReleaseDC(std::ptr::null_mut(), dc);
        for family in ["Segoe UI Variable Text", "Segoe UI"] {
            if fonts.contains(family) {
                *face().write().unwrap() = family.into();
                break;
            }
        }
    }
}
unsafe extern "system" fn collect_font(
    lf: *const windows_sys::Win32::Graphics::Gdi::LOGFONTW,
    _: *const windows_sys::Win32::Graphics::Gdi::TEXTMETRICW,
    _: u32,
    data: isize,
) -> i32 {
    let set = &mut *(data as *mut std::collections::BTreeSet<String>);
    let a = &(*lf).lfFaceName;
    let n = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    let name = String::from_utf16_lossy(&a[..n]);
    if !name.starts_with('@')
        && (*lf).lfCharSet != windows_sys::Win32::Graphics::Gdi::SYMBOL_CHARSET
    {
        set.insert(name);
    }
    1
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe extern "system" fn font_proc(
    h: windows_sys::Win32::Foundation::HWND,
    m: u32,
    w: usize,
    l: isize,
) -> isize {
    use windows_sys::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    match m {
        WM_COMMAND => {
            let id = w & 0xffff;
            if id == 422 {
                let c = GetDlgItem(h, 420);
                let n = SendMessageW(
                    c,
                    CB_FINDSTRINGEXACT,
                    usize::MAX,
                    wide("Segoe UI Variable Text").as_ptr() as isize,
                );
                if n >= 0 {
                    SendMessageW(c, CB_SETCURSEL, n as usize, 0);
                    SendMessageW(h, WM_COMMAND, 420 | (CBN_SELCHANGE as usize) << 16, 0);
                }
            }
            if id == 401 || id == 402 || id == 2 {
                SetPropW(
                    h,
                    wide("answer").as_ptr(),
                    if id == 401 { 1usize as _ } else { 2usize as _ },
                );
            }
            if id == 420 && w >> 16 == CBN_SELCHANGE as usize {
                let c = GetDlgItem(h, 420);
                let n = SendMessageW(c, CB_GETCURSEL, 0, 0);
                let mut b = [0u16; 128];
                if n >= 0 {
                    SendMessageW(c, CB_GETLBTEXT, n as usize, b.as_mut_ptr() as isize);
                    let f = CreateFontW(
                        -24,
                        0,
                        0,
                        0,
                        400,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET as _,
                        0,
                        0,
                        CLEARTYPE_QUALITY as _,
                        0,
                        b.as_ptr(),
                    );
                    let old = GetPropW(h, wide("preview-font").as_ptr());
                    SendMessageW(GetDlgItem(h, 421), WM_SETFONT, f as usize, 1);
                    if !old.is_null() {
                        DeleteObject(old);
                    }
                    SetPropW(h, wide("preview-font").as_ptr(), f);
                }
            }
            0
        }
        WM_CLOSE => {
            SetPropW(h, wide("answer").as_ptr(), 2usize as _);
            0
        }
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
            let mut r = std::mem::zeroed();
            GetClientRect(h, &mut r);
            let br = CreateSolidBrush(crate::ui::bg());
            FillRect(dc, &r, br);
            DeleteObject(br);
            crate::ui::end_buffer(h, &ps, dc, b);
            0
        }
        WM_CTLCOLORSTATIC => {
            SetTextColor(w as _, crate::ui::text_color());
            SetBkColor(w as _, crate::ui::bg());
            static BRUSH: OnceLock<usize> = OnceLock::new();
            *BRUSH.get_or_init(|| CreateSolidBrush(crate::ui::bg()) as usize) as isize
        }
        _ => crate::ui::handle(h, m, w, l).unwrap_or_else(|| DefWindowProcW(h, m, w, l)),
    }
}
pub unsafe fn choose(
    parent: windows_sys::Win32::Foundation::HWND,
    root: &Path,
) -> Result<Option<String>, String> {
    use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    let mut fonts = std::collections::BTreeSet::new();
    let dc = GetDC(parent);
    let lf = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..std::mem::zeroed()
    };
    EnumFontFamiliesExW(
        dc,
        &lf,
        Some(collect_font),
        &mut fonts as *mut _ as isize,
        0,
    );
    ReleaseDC(parent, dc);
    let current = font_name();
    fonts.insert(current.clone());
    fonts.insert("Segoe UI Variable Text".into());
    let fonts: Vec<String> = fonts.into_iter().collect();
    let class = wide("ZimClipFontDialog");
    let inst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(font_proc),
        hInstance: inst,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let mut r = RECT::default();
    GetWindowRect(parent, &mut r);
    let h = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide("App font").as_ptr(),
        WS_POPUP | WS_CLIPCHILDREN,
        r.left + 100,
        r.top + 180,
        650,
        300,
        parent,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    crate::ui::clip_round(h, 24);
    crate::ui::titlebar(h);
    let font = SendMessageW(GetDlgItem(parent, 14), WM_GETFONT, 0, 0);
    let add =
        |cls: &str, text: &str, id: usize, x: i32, y: i32, width: i32, height: i32, style: u32| {
            let c = CreateWindowExW(
                0,
                wide(cls).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                x,
                y,
                width,
                height,
                h,
                id as _,
                inst,
                std::ptr::null(),
            );
            SendMessageW(c, WM_SETFONT, font as usize, 1);
            c
        };
    add("STATIC", "App font", 0, 24, 20, 580, 28, 0);
    add(
        "STATIC",
        "Choose an installed font. Sizes stay fixed. Applies after app restart.",
        0,
        24,
        54,
        602,
        28,
        0,
    );
    let combo = add(
        "COMBOBOX",
        "",
        420,
        24,
        96,
        602,
        260,
        CBS_DROPDOWNLIST as u32 | CBS_OWNERDRAWFIXED as u32 | CBS_HASSTRINGS as u32 | WS_TABSTOP,
    );
    for n in &fonts {
        SendMessageW(combo, CB_ADDSTRING, 0, wide(n).as_ptr() as isize);
    }
    let index = fonts.iter().position(|s| s == &current).unwrap_or(0);
    SendMessageW(combo, CB_SETCURSEL, index, 0);
    crate::ui::dropdown(combo);
    add(
        "STATIC",
        "Keep the moment. Leave the rest. 0123456789",
        421,
        24,
        151,
        602,
        46,
        0,
    );
    for (text, id, x) in [
        ("Reset default", 422, 24),
        ("Cancel", 402, 308),
        ("Apply font", 401, 466),
    ] {
        let c = add(
            "BUTTON",
            text,
            id,
            x,
            226,
            150,
            42,
            BS_OWNERDRAW as u32 | WS_TABSTOP,
        );
        crate::ui::interactive(c);
    }
    SendMessageW(h, WM_COMMAND, 420 | (CBN_SELCHANGE as usize) << 16, 0);
    windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(parent, 0);
    ShowWindow(h, SW_SHOW);
    SetForegroundWindow(h);
    let mut msg = MSG::default();
    loop {
        if !GetPropW(h, wide("answer").as_ptr()).is_null() {
            break;
        }
        if PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let yes = GetPropW(h, wide("answer").as_ptr()) as usize == 1;
    let i = SendMessageW(combo, CB_GETCURSEL, 0, 0) as usize;
    let selected = fonts.get(i).cloned();
    let preview = GetPropW(h, wide("preview-font").as_ptr());
    DestroyWindow(h);
    if !preview.is_null() {
        DeleteObject(preview);
    }
    windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    if yes {
        if let Some(name) = selected {
            std::fs::write(root.join("font.txt"), &name).map_err(|e| e.to_string())?;
            *face().write().unwrap() = name.clone();
            return Ok(Some(name));
        }
    }
    Ok(None)
}
#[derive(Clone, Copy)]
pub struct Item {
    pub h: windows_sys::Win32::Foundation::HWND,
    pub r: windows_sys::Win32::Foundation::RECT,
}
pub unsafe fn snapshot(parent: windows_sys::Win32::Foundation::HWND) -> Vec<Item> {
    use windows_sys::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
    let mut out = Vec::new();
    let mut h = GetWindow(parent, GW_CHILD);
    while !h.is_null() {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut r);
        let mut p = POINT {
            x: r.left,
            y: r.top,
        };
        windows_sys::Win32::Graphics::Gdi::ScreenToClient(parent, &mut p);
        r.right = p.x + r.right - r.left;
        r.bottom = p.y + r.bottom - r.top;
        r.left = p.x;
        r.top = p.y;
        out.push(Item { h, r });
        h = GetWindow(h, GW_HWNDNEXT);
    }
    out
}
pub unsafe fn put(h: windows_sys::Win32::Foundation::HWND, x: i32, y: i32, w: i32, z: i32) {
    windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
        h,
        std::ptr::null_mut(),
        x,
        y,
        w,
        z,
        windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
    );
}
pub unsafe fn apply(
    parent: windows_sys::Win32::Foundation::HWND,
    items: &[Item],
    mode: usize,
    settings: bool,
    rows: &[windows_sys::Win32::Foundation::HWND],
    large: bool,
) {
    use windows_sys::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    // Sidebar (4) is a Polished-only layout. Legacy and Settings use the Balanced geometry.
    let mode = if mode == 4 && (!crate::polished::enabled() || settings) { 0 } else { mode };
    crate::polished::set_sidebar(mode == 4);
    SetPropW(parent, wide("display-layout").as_ptr(), mode as _);
    // Marks the main window even when layout 0 / settings off store a NULL value.
    SetPropW(parent, wide("display-ready").as_ptr(), 1usize as _);
    SetPropW(
        parent,
        wide("display-settings").as_ptr(),
        settings as usize as _,
    );
    for key in ["rightheading", "righthint"] {
        let h = GetPropW(parent, wide(&format!("slot-{key}")).as_ptr()) as HWND;
        ShowWindow(
            h,
            if settings || mode != 1 {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
    }
    let slot = |key: &str| GetPropW(parent, wide(&format!("slot-{key}")).as_ptr()) as HWND;
    if !settings {
        ShowWindow(slot("privacy"), SW_SHOW);
    }
    // Details state is owned by the "compact-expanded" property (set by the Details button),
    // so page and layout changes cannot desync it from what is visible.
    let expanded = GetPropW(parent, wide("compact-expanded").as_ptr()) as usize != 0;
    SetPropW(
        parent,
        wide("compact-expanded").as_ptr(),
        expanded as usize as _,
    );
    let polished = crate::polished::enabled();
    let (mut w, mut h) = if settings {
        (
            match mode {
                1 => 610,
                2 => 1240,
                _ => if polished { 1120 } else { 970 },
            },
            720,
        )
    } else {
        match mode {
            1 => (610, 720),
            2 => (1240, 720),
            3 => if polished { (1120, 750) } else { (970, 740) },
            _ => if polished { (1120, 750) } else { (970, 740) },
        }
    };
    let mut work = RECT::default();
    let monitor = windows_sys::Win32::Graphics::Gdi::MonitorFromWindow(
        parent,
        windows_sys::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
    );
    let mut info = windows_sys::Win32::Graphics::Gdi::MONITORINFO {
        cbSize: std::mem::size_of::<windows_sys::Win32::Graphics::Gdi::MONITORINFO>() as u32,
        ..std::mem::zeroed()
    };
    if windows_sys::Win32::Graphics::Gdi::GetMonitorInfoW(monitor, &mut info) != 0 {
        work = info.rcWork;
    } else {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as _, 0);
    }
    // Never taller or wider than the visible work area (small laptops, taskbar): the
    // layout anchors the result row and footer to the client bottom, so a shorter window
    // keeps every control reachable instead of pushing the footer off screen.
    h = h.min((work.bottom - work.top).max(400));
    w = w.min((work.right - work.left).max(640));
    let mut current = RECT::default();
    GetWindowRect(parent, &mut current);
    let x = current
        .left
        .clamp(work.left, (work.right - w).max(work.left));
    let y = current.top.clamp(work.top, (work.bottom - h).max(work.top));
    SetWindowPos(parent, std::ptr::null_mut(), x, y, w, h, SWP_NOZORDER);
    for i in items {
        let r = i.r;
        put(i.h, r.left, r.top, r.right - r.left, r.bottom - r.top);
    }
    let slot = |key: &str| GetPropW(parent, wide(&format!("slot-{key}")).as_ptr()) as HWND;
    if settings {
        ShowWindow(slot("heading"), SW_SHOW);
    } else if mode == 1 {
        for i in items {
            if i.r.left >= 606 && ![4, 5, 14].contains(&GetDlgCtrlID(i.h)) {
                ShowWindow(i.h, SW_HIDE);
            }
        }
        put(GetDlgItem(parent, 14), 400, 24, 160, 38);
        put(slot("heading"), 66, 20, 320, 42);
        let extra = if expanded { 70 } else { 0 };
        put(slot("status"), 48, 156, 514, 38);
        put(slot("simplehint"), 48, 206, 514, 30);
        put(GetDlgItem(parent, 1), 48, 250, 514, 44);
        put(GetDlgItem(parent, 10), 48, 306, 248, 38);
        put(GetDlgItem(parent, 43), 310, 306, 252, 38);
        put(slot("warning"), 48, 362, 514, 48);
        put(slot("details"), 48, 362, 514, 44);
        put(slot("audio"), 48, 412, 514, 64);
        put(GetDlgItem(parent, 40), 48, 424 + extra, 150, 38);
        put(GetDlgItem(parent, 44), 310, 424 + extra, 252, 38);
        put(slot("privacy"), 48, 474 + extra, 514, 24);
        put(slot("result"), 26, 534 + extra, 330, 40);
        put(slot("open"), 374, 534 + extra, 188, 38);
        ShowWindow(slot("open"), SW_SHOW);
        put(slot("footer"), 26, 586 + extra, 536, 24);
        put(GetDlgItem(parent, 4), 26, 630 + extra, 150, 38);
        put(GetDlgItem(parent, 5), 190, 630 + extra, 150, 38);
        ShowWindow(GetDlgItem(parent, 4), SW_SHOW);
        ShowWindow(GetDlgItem(parent, 5), SW_SHOW);
    } else if mode == 2 {
        for i in items {
            let r = i.r;
            if r.left >= 606 && GetDlgCtrlID(i.h) != 14 {
                put(i.h, r.left - 190, r.top, r.right - r.left, r.bottom - r.top);
            } else if (100..604).contains(&r.top) {
                put(
                    i.h,
                    r.left,
                    r.top,
                    (r.right - r.left).min(320),
                    r.bottom - r.top,
                );
            }
        }
        put(GetDlgItem(parent, 10), 48, 330, 320, 38);
        put(GetDlgItem(parent, 43), 48, 378, 320, 38);
        put(slot("simplehint"), 48, 230, 320, 42);
        put(slot("warning"), 48, 430, 320, 60);
        put(slot("privacy"), 48, 554, 320, 40);
        put(GetDlgItem(parent, 40), 48, 510, 120, 38);
        put(GetDlgItem(parent, 44), 180, 510, 188, 38);
        put(GetDlgItem(parent, 14), 1006, 24, 192, 38);
        put(slot("result"), 26, 618, 350, 40);
        if expanded {
            put(slot("status"), 48, 146, 320, 36);
            put(slot("simplehint"), 48, 190, 320, 42);
            put(GetDlgItem(parent, 1), 48, 240, 320, 44);
            put(GetDlgItem(parent, 10), 48, 296, 320, 38);
            put(GetDlgItem(parent, 43), 48, 342, 320, 38);
            put(slot("details"), 48, 394, 320, 44);
            put(slot("audio"), 48, 442, 320, 72);
            put(GetDlgItem(parent, 40), 48, 520, 120, 38);
            put(GetDlgItem(parent, 44), 180, 520, 188, 38);
            put(slot("privacy"), 48, 566, 320, 32);
        }
    } else if mode == 3 {
        put(GetDlgItem(parent, 14), 736, 24, 192, 38);
        put(slot("status"), 48, 182, 400, 38);
        put(slot("simplehint"), 48, 230, 400, 48);
        put(GetDlgItem(parent, 1), 500, 182, 404, 44);
        put(GetDlgItem(parent, 10), 500, 240, 195, 38);
        put(GetDlgItem(parent, 43), 709, 240, 195, 38);
        put(slot("warning"), 48, 294, 514, 58);
        put(slot("privacy"), 48, 404, 560, 24);
        put(GetDlgItem(parent, 40), 48, 358, 150, 38);
        put(GetDlgItem(parent, 44), 650, 358, 254, 38);
        put(slot("details"), 48, 292, 514, 42);
        put(slot("audio"), 48, 338, 514, 64);
        put(slot("rightheading"), 48, 492, 210, 24);
        put(slot("righthint"), 48, 520, 510, 24);
        put(GetDlgItem(parent, 36), 806, 486, 98, 30);
        put(slot("open"), 650, 742, 254, 38);
        put(slot("result"), 26, 804, 562, 40);
        put(slot("footer"), 26, 856, 902, 24);
        put(GetDlgItem(parent, 4), 606, 804, 142, 38);
        put(GetDlgItem(parent, 5), 762, 804, 166, 38);
    }
    if !settings && mode == 1 && expanded {
        put(slot("status"), 48, 140, 514, 38);
        put(slot("simplehint"), 48, 184, 514, 30);
        put(GetDlgItem(parent, 1), 48, 222, 514, 44);
        put(GetDlgItem(parent, 10), 48, 278, 248, 38);
        put(GetDlgItem(parent, 43), 310, 278, 252, 38);
        put(slot("details"), 48, 334, 514, 44);
        put(slot("audio"), 48, 384, 514, 64);
        put(GetDlgItem(parent, 40), 48, 462, 150, 38);
        put(GetDlgItem(parent, 44), 310, 462, 252, 38);
        put(slot("privacy"), 48, 512, 514, 24);
        put(slot("result"), 26, 566, 330, 40);
        put(slot("open"), 374, 566, 188, 38);
        put(slot("footer"), 26, 618, 536, 24);
        put(GetDlgItem(parent, 4), 26, 650, 150, 38);
        put(GetDlgItem(parent, 5), 190, 650, 150, 38);
    }
    if !settings && mode == 0 && !expanded {
        put(slot("warning"), 48, 392, 514, 54);
        put(GetDlgItem(parent, 40), 48, 458, 150, 38);
        put(GetDlgItem(parent, 44), 310, 458, 252, 38);
    }
    if !settings && mode == 3 {
        put(slot("status"), 48, 146, 400, 38);
        put(slot("simplehint"), 48, 194, 400, 44);
        put(GetDlgItem(parent, 1), 500, 146, 404, 44);
        put(GetDlgItem(parent, 10), 500, 204, 195, 38);
        put(GetDlgItem(parent, 43), 709, 204, 195, 38);
        put(slot("warning"), 48, 246, 514, 42);
        put(GetDlgItem(parent, 40), 48, 292, 150, 32);
        put(GetDlgItem(parent, 44), 650, 292, 254, 32);
        ShowWindow(slot("privacy"), SW_HIDE);
        if expanded {
            put(slot("details"), 48, 240, 550, 48);
            put(slot("audio"), 48, 292, 550, 64);
            put(GetDlgItem(parent, 40), 650, 292, 254, 32);
            put(GetDlgItem(parent, 44), 650, 334, 254, 32);
        }
        let shift = if expanded { 40 } else { 0 };
        put(slot("rightheading"), 48, 380 + shift, 210, 24);
        put(slot("righthint"), 48, 408 + shift, 510, 24);
        put(GetDlgItem(parent, 36), 806, 374 + shift, 98, 30);
        put(slot("open"), 650, 542, 254, 38);
        put(slot("result"), 26, 618, 562, 40);
        put(slot("footer"), 26, 666, 902, 24);
        put(GetDlgItem(parent, 4), 606, 618, 142, 38);
        put(GetDlgItem(parent, 5), 762, 618, 166, 38);
    }
    let count = GetPropW(parent, wide("library-count").as_ptr()) as usize;
    let offset = GetPropW(parent, wide("library-offset").as_ptr()) as usize;
    for (i, row) in rows.iter().enumerate() {
        ShowWindow(
            *row,
            if !settings && mode != 1 && i < count.saturating_sub(offset).min(rows.len()) {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
    }
    if !settings {
        for (i, row) in rows.iter().enumerate() {
            let (x, y, w, h) = match mode {
                2 => (
                    440 + (i % 2) as i32 * 378,
                    180 + (i / 2) as i32 * if large { 112 } else { 66 },
                    354,
                    if large { 100 } else { 54 },
                ),
                3 => (
                    48 + (i % 3) as i32 * 292,
                    (if expanded { 470 } else { 442 })
                        + (i / 3) as i32
                            * if expanded {
                                54
                            } else if large {
                                72
                            } else {
                                48
                            },
                    274,
                    if expanded {
                        50
                    } else if large {
                        64
                    } else {
                        40
                    },
                ),
                _ => (
                    630,
                    180 + i as i32 * if large { 72 } else { 58 },
                    274,
                    if large { 64 } else { 38 },
                ),
            };
            put(*row, x, y, w, h);
        }
        if mode == 2 {
            put(GetDlgItem(parent, 37), 1190, 180, 12, 340);
        }
        if mode == 3 {
            put(
                GetDlgItem(parent, 37),
                916,
                if expanded { 470 } else { 444 },
                12,
                104,
            );
        }
    }

    // Library toolbar has its own search row and paired sort/tag filters.
    let lib_visible = !settings && mode != 1;
    for id in [76, 77, 78] {
        ShowWindow(
            GetDlgItem(parent, id),
            if lib_visible { SW_SHOW } else { SW_HIDE },
        );
    }
    if lib_visible {
        let (x, y, width) = match mode {
            2 => (440, 112, 732),
            3 => (48, if expanded { 404 } else { 374 }, 856),
            _ => (630, 112, 274),
        };
        put(
            GetDlgItem(parent, 76),
            x,
            y,
            if mode == 3 { 400 } else { width },
            32,
        );
        let cy = if mode == 3 { y } else { y + 40 };
        let cx = if mode == 3 { x + 414 } else { x };
        let cw = if mode == 3 { 210 } else { (width - 8) / 2 };
        put(GetDlgItem(parent, 77), cx, cy, cw, 260);
        put(GetDlgItem(parent, 78), cx + cw + 8, cy, cw, 260);
        ShowWindow(slot("rightheading"), SW_HIDE);
        put(
            slot("righthint"),
            x,
            if mode == 3 { y + 40 } else { y + 78 },
            width,
            24,
        );
        for (i, row) in rows.iter().enumerate() {
            let (rx, ry, rw, rh) = match mode {
                2 => (
                    440 + (i % 2) as i32 * 378,
                    216 + (i / 2) as i32 * if large { 100 } else { 66 },
                    354,
                    if large { 88 } else { 54 },
                ),
                3 => (
                    48 + (i % 3) as i32 * 292,
                    if expanded { 470 } else { 444 } + (i / 3) as i32 * 50,
                    274,
                    48,
                ),
                _ => (
                    630,
                    216 + i as i32 * if large { 60 } else { 56 },
                    274,
                    if large { 54 } else { 48 },
                ),
            };
            put(*row, rx, ry, rw, rh);
        }
        if mode != 3 {
            put(
                GetDlgItem(parent, 37),
                if mode == 2 { 1190 } else { 910 },
                216,
                12,
                if mode == 2 { 306 } else { 292 },
            );
        }
        put(
            GetDlgItem(parent, 36),
            if mode == 2 {
                1074
            } else if mode == 3 {
                806
            } else {
                806
            },
            if mode == 3 {
                y + 44
            } else if mode == 2 {
                554
            } else {
                518
            },
            98,
            30,
        );
    }
    ShowWindow(
        GetDlgItem(parent, 36),
        if lib_visible && mode != 3 {
            SW_SHOW
        } else {
            SW_HIDE
        },
    );
    ShowWindow(
        GetDlgItem(parent, 37),
        if lib_visible { SW_SHOW } else { SW_HIDE },
    );
    ShowWindow(
        slot("righthint"),
        if lib_visible { SW_SHOW } else { SW_HIDE },
    );
    let refresh = GetDlgItem(parent, 81);
    ShowWindow(refresh, if lib_visible { SW_SHOW } else { SW_HIDE });
    ShowWindow(
        GetDlgItem(parent, 85),
        if lib_visible { SW_SHOW } else { SW_HIDE },
    );
    if lib_visible {
        let (x, y) = match mode {
            2 => (900, 554),
            3 => (48, if expanded { 572 } else { 548 }),
            _ => (630, 518),
        };
        put(refresh, x, y, 108, 30);
        put(GetDlgItem(parent, 85), x + 116, y, 54, 30);
    }
    for (id, x) in [(86, 48), (87, 310)] {
        let c = GetDlgItem(parent, id);
        ShowWindow(
            c,
            if !settings && !expanded {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
        let (left, y, width) = match mode {
            2 => (48, 474, if id == 86 { 154 } else { 154 }),
            1 => (x, 394, if id == 86 { 248 } else { 252 }),
            3 => (if id == 86 { 486 } else { 688 }, 254, 194),
            _ => (x, 460, if id == 86 { 248 } else { 252 }),
        };
        put(
            c,
            if mode == 2 && id == 87 { 210 } else { left },
            y,
            width,
            30,
        );
    }
    if !settings && !expanded {
        if mode == 1 {
            put(slot("warning"), 48, 354, 514, 34);
            put(GetDlgItem(parent, 40), 48, 432, 150, 38);
            put(GetDlgItem(parent, 44), 310, 432, 252, 38);
        }
        if mode == 0 {
            put(GetDlgItem(parent, 40), 48, 504, 150, 38);
            put(GetDlgItem(parent, 44), 310, 504, 252, 38);
        }
        if mode == 2 {
            put(slot("warning"), 48, 426, 320, 42);
        }
        if mode == 3 {
            put(slot("warning"), 48, 236, 420, 52);
        }
    }
    if settings {
        ShowWindow(GetDlgItem(parent, crate::ctl_ids::HOME_SUBPAGE as i32), SW_HIDE);
    }
    crate::settings_view::layout(parent, settings);
    let selecting = crate::library::state().lock().unwrap().active;
    let visible = !settings && mode != 1;
    ShowWindow(slot("filter"), if visible { SW_SHOW } else { SW_HIDE });
    ShowWindow(slot("select"), if visible { SW_SHOW } else { SW_HIDE });
    ShowWindow(
        slot("delete"),
        if visible && selecting {
            SW_SHOW
        } else {
            SW_HIDE
        },
    );
    if visible {
        let (x, y, total) = match mode {
            2 => (440, 550, 354),
            3 => (650, if expanded { 572 } else { 548 }, 254),
            _ => (630, 550, 274),
        };
        let button_height = if mode == 3 { 28 } else { 38 };
        if selecting {
            ShowWindow(slot("open"), SW_HIDE);
            put(slot("delete"), x, y, total - 84, button_height);
            put(slot("select"), x + total - 76, y, 76, button_height);
        } else {
            ShowWindow(slot("open"), SW_SHOW);
            put(slot("open"), x, y, total - 124, button_height);
            put(slot("select"), x + total - 116, y, 116, button_height);
        }
    }
    // Coordinates must use the client area: Windows title bars consume part
    // of the requested outer height, unlike some compatibility desktops.
    if !settings {
        let mut client = RECT::default();
        GetClientRect(parent, &mut client);
        if mode == 1 {
            put(GetDlgItem(parent, 4), 26, client.bottom - 76, 150, 38);
            put(GetDlgItem(parent, 5), 190, client.bottom - 76, 150, 38);
            if expanded {
                put(slot("result"), 26, client.bottom - 122, 330, 40);
                put(slot("open"), 374, client.bottom - 122, 188, 38);
            }
        }
        if mode != 1 {
            put(
                slot("result"),
                26,
                client.bottom - 78,
                if mode == 2 { 350 } else { 562 },
                40,
            );
            put(
                GetDlgItem(parent, 4),
                if mode == 2 { 416 } else { 606 },
                client.bottom - 78,
                142,
                38,
            );
            put(
                GetDlgItem(parent, 5),
                if mode == 2 { 572 } else { 762 },
                client.bottom - 78,
                166,
                38,
            );
        }
        put(
            slot("footer"),
            26,
            client.bottom - 30,
            (client.right - 52).max(200),
            24,
        );
    }
    crate::replay_design::layout(parent, mode, settings, expanded, rows, large);
    windows_sys::Win32::Graphics::Gdi::RedrawWindow(
        parent,
        std::ptr::null(),
        std::ptr::null_mut(),
        windows_sys::Win32::Graphics::Gdi::RDW_INVALIDATE
            | windows_sys::Win32::Graphics::Gdi::RDW_ALLCHILDREN
            | windows_sys::Win32::Graphics::Gdi::RDW_UPDATENOW,
    );
}
