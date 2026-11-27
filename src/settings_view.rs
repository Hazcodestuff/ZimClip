use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_FOCUS};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
};
const K_PAUSE: i32 = crate::ctl_ids::PAUSE_KEY as i32;
const K_APPLY_PAUSE: i32 = crate::ctl_ids::APPLY_PAUSE as i32;
const K_TPL: i32 = crate::ctl_ids::TEMPLATE_EDIT as i32;
const K_APPLY_TPL: i32 = crate::ctl_ids::APPLY_TEMPLATE as i32;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[derive(Clone)]
struct Control {
    id: i32,
    h: usize,
    x: i32,
    y: i32,
    w: i32,
    z: i32,
    category: usize,
}
#[derive(Default, Clone)]
struct State {
    view: usize,
    bar: usize,
    controls: Vec<Control>,
    offset: i32,
    height: i32,
    total: i32,
    mode: usize,
    width: i32,
    category: usize,
}
/// Set after a confirmed settings import: the page shows a short lock explanation.
/// 0 normal, 1 locked after an import, 2 imports blocked by an unfinished earlier import.
static IMPORT_STATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
fn backup_status() -> &'static std::sync::Mutex<String> {
    static S: std::sync::OnceLock<std::sync::Mutex<String>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}
fn name_example() -> &'static std::sync::Mutex<String> {
    static S: std::sync::OnceLock<std::sync::Mutex<String>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}
fn tpl_feedback() -> &'static std::sync::Mutex<(String, String, bool)> {
    static S: std::sync::OnceLock<std::sync::Mutex<(String, String, bool)>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}
fn shortcut_msgs() -> &'static std::sync::Mutex<[(String, bool); 2]> {
    static S: std::sync::OnceLock<std::sync::Mutex<[(String, bool); 2]>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}
/// Result of the last Apply under the Save (0) or Pause (1) shortcut box. Shown on the
/// Capture page itself, because the Home status line is hidden while Settings is open.
pub unsafe fn set_shortcut_msg(which: usize, msg: &str, error: bool) {
    shortcut_msgs().lock().unwrap()[which.min(1)] = (msg.to_string(), error);
    let v = state().lock().unwrap().view;
    if v != 0 {
        InvalidateRect(v as HWND, std::ptr::null(), 0);
    }
}
fn pause_label() -> &'static std::sync::Mutex<String> {
    static S: std::sync::OnceLock<std::sync::Mutex<String>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}
/// Live preview of the template box and the plain-words problem with it, if any.
pub unsafe fn set_template_feedback(preview: &str, msg: &str, error: bool) {
    *tpl_feedback().lock().unwrap() = (preview.to_string(), msg.to_string(), error);
    let v = state().lock().unwrap().view;
    if v != 0 {
        InvalidateRect(v as HWND, std::ptr::null(), 0);
    }
}
/// Current pause shortcut, shown in the Shortcuts text.
pub unsafe fn set_pause_label(label: &str) {
    *pause_label().lock().unwrap() = label.to_string();
    let v = state().lock().unwrap().view;
    if v != 0 {
        InvalidateRect(v as HWND, std::ptr::null(), 0);
    }
}
/// Example clip file name shown under the File names button.
pub unsafe fn set_name_example(example: &str) {
    *name_example().lock().unwrap() = example.to_string();
    let v = state().lock().unwrap().view;
    if v != 0 {
        InvalidateRect(v as HWND, std::ptr::null(), 0);
    }
}
/// Result of the last export/import, shown on the Files / app page where the buttons are.
pub unsafe fn set_backup_status(msg: &str) {
    *backup_status().lock().unwrap() = msg.to_string();
    let v = state().lock().unwrap().view;
    if v != 0 {
        scroll(0);
        InvalidateRect(v as HWND, std::ptr::null(), 0);
    }
}
pub unsafe fn show_import_lock() {
    set_import_state(1);
}
pub unsafe fn set_import_state(v: u8) {
    IMPORT_STATE.store(v, std::sync::atomic::Ordering::Relaxed);
    let v = state().lock().unwrap().view;
    if v != 0 {
        scroll(0);
        InvalidateRect(v as HWND, std::ptr::null(), 0);
    }
}
/// Selected Settings page: 0 Capture, 1 Appearance, 2 Files / app.
pub fn current_category() -> usize {
    state().lock().unwrap().category
}
fn state() -> &'static std::sync::Mutex<State> {
    static S: std::sync::OnceLock<std::sync::Mutex<State>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}
unsafe extern "system" fn child_proc(
    h: HWND,
    m: u32,
    w: usize,
    l: isize,
    _: usize,
    _: usize,
) -> isize {
    if m == WM_MOUSEWHEEL {
        return SendMessageW(GetParent(h), m, w, l);
    }
    if m == WM_SETFOCUS {
        let view = state().lock().unwrap().view as HWND;
        if GetParent(h) == view {
            ensure(h);
        }
    }
    DefSubclassProc(h, m, w, l)
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_MOUSEWHEEL => {
            scroll(if (w >> 16) as i16 > 0 { -96 } else { 96 });
            0
        }
        WM_VSCROLL => {
            let code = w & 65535;
            match code {
                0 => scroll(-48),
                1 => scroll(48),
                2 => scroll(-300),
                3 => scroll(300),
                6 => scroll(-2000),
                7 => scroll(2000),
                4 | 5 => {
                    let mut i = SCROLLINFO {
                        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                        fMask: SIF_TRACKPOS,
                        ..std::mem::zeroed()
                    };
                    GetScrollInfo(h, SB_VERT, &mut i);
                    let old = state().lock().unwrap().offset;
                    scroll(i.nTrackPos - old);
                }
                _ => {}
            }
            0
        }
        WM_LBUTTONDOWN => {
            preview_click(h, l);
            0
        }
        WM_DRAWITEM if (300..319).contains(&(w as i32)) => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            draw_choice(d);
            1
        }
        WM_TIMER if w == crate::ui::motion::PILL_TIMER => {
            let (_, moving) = crate::ui::motion::pill();
            let parent = GetParent(h);
            for i in 0..3 {
                InvalidateRect(GetDlgItem(parent, 82 + i), std::ptr::null(), 0);
            }
            // The parent paints the gaps between the buttons, so the pill stays one piece.
            let band = crate::polished::pill_band(parent);
            InvalidateRect(parent, &band, 0);
            if !moving || !crate::ui::motion::allowed(h) {
                KillTimer(h, crate::ui::motion::PILL_TIMER);
            }
            0
        }
        WM_COMMAND if (300..319).contains(&((w & 65535) as i32)) => {
            choose_index(h, (w & 65535) as i32);
            0
        }
        WM_COMMAND if (82..=84).contains(&(w & 65535)) => {
            jump((w & 65535) - 82);
            0
        }
        WM_COMMAND | WM_HSCROLL | WM_DRAWITEM | WM_MEASUREITEM => {
            SendMessageW(GetParent(h), m, w, l)
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX => {
            crate::ui::handle(h, m, w, l).unwrap_or_else(|| DefWindowProcW(h, m, w, l))
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            FillRect(dc, &r, crate::ui::style::brush(crate::ui::bg()));
            paint_page(h, dc, r);
            crate::ui::end_buffer(h, &ps, dc, b);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
pub unsafe fn init(parent: HWND, controls: &[HWND], font: HFONT) {
    crate::ui::warm_settings();
    let cls = wide("ZimClipSettingsScroll");
    let inst = GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let h = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        cls.as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_CLIPCHILDREN,
        24,
        112,
        904,
        440,
        parent,
        74usize as _,
        inst,
        std::ptr::null(),
    );
    let mut s = State::default();
    s.category = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("settings-page.txt")))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0)
        .min(2);
    let barcls = wide("ZimClipSettingsBar");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(bar_proc),
        hInstance: inst,
        lpszClassName: barcls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let bar = CreateWindowExW(
        0,
        barcls.as_ptr(),
        std::ptr::null(),
        WS_CHILD,
        888,
        0,
        12,
        440,
        parent,
        75usize as _,
        inst,
        std::ptr::null(),
    );
    s.bar = bar as usize;
    s.view = h as usize;
    s.height = 440;
    let mut add = |c: HWND, x: i32, y: i32, w: i32, z: i32| {
        SetParent(c, h);
        SetWindowSubclass(c, Some(child_proc), 77, 0);
        s.controls.push(Control {
            id: GetDlgCtrlID(c),
            h: c as usize,
            x,
            y,
            w,
            z,
            category: 0,
        });
    };
    for &c in controls {
        let id = GetDlgCtrlID(c);
        let mut t = vec![0u16; GetWindowTextLengthW(c) as usize + 1];
        let n = GetWindowTextW(c, t.as_mut_ptr(), t.len() as i32);
        let text = String::from_utf16_lossy(&t[..n as usize]);
        let pos = match id {
            20 => (24, 94, 520, 38),
            23 => (568, 94, 264, 38),
            21 => (24, 184, 808, 38),
            7 => (24, 248, 392, 42),
            12 => (440, 248, 392, 42),
            26 => (24, 344, 700, 32),
            27 => (24, 436, 700, 32),
            30 => (24, 548, 392, 42),
            42 => (440, 548, 392, 42),
            62 => (24, 610, 392, 42),
            73 => (440, 610, 392, 42),
            79 => (24, 1200, 392, 42),
            80 => (440, 1200, 392, 42),
            31 => (24, 768, 392, 38),
            22 => (440, 768, 392, 38),
            33 => (24, 854, 392, 42),
            32 => (440, 854, 392, 42),
            34 => (24, 926, 392, 42),
            35 => (440, 926, 392, 42),
            x if x == K_PAUSE => (24, 1000, 392, 42),
            x if x == K_APPLY_PAUSE => (440, 1000, 392, 42),
            x if x == K_TPL => (24, 1200, 392, 36),
            x if x == K_APPLY_TPL => (440, 1200, 130, 38),
            41 => (24, 990, 808, 42),
            45 => (24, 1128, 392, 38),
            46 => (440, 1128, 392, 42),
            _ => match text.as_str() {
                "Video changes reset the unsaved buffer." => (24, 22, 808, 24),
                "Resolution" => (24, 62, 520, 24),
                "Capture FPS" => (568, 62, 264, 24),
                "Capture path" => (24, 152, 808, 24),
                "System gain" => (24, 310, 700, 24),
                "Microphone gain" => (24, 402, 700, 24),
                "+0 dB" => {
                    let mut r = RECT::default();
                    GetWindowRect(c, &mut r);
                    let mut p = POINT {
                        x: r.left,
                        y: r.top,
                    };
                    ScreenToClient(parent, &mut p);
                    (748, if p.y < 470 { 344 } else { 436 }, 84, 26)
                }
                "Canvas theme" => (24, 736, 392, 24),
                "Accent color" => (440, 736, 392, 24),
                "Save shortcut" => (24, 828, 392, 24),
                "Layout" => (24, 1090, 392, 24),
                "Font, fixed readable sizes" => (440, 1090, 392, 24),
                _ => (24, 1200, 808, 24),
            },
        };
        add(
            c,
            pos.0,
            if pos.1 < 690 { pos.1 + 24 } else { pos.1 },
            pos.2,
            pos.3,
        );
    }
    for c in &mut s.controls {
        let id = GetDlgCtrlID(c.h as HWND);
        c.category = match id {
            31 | 22 | 41 | 45 | 46 | 91 | 92 | 95 => 1,
            x if x == crate::ctl_ids::TAGLINE_EDIT as i32 => 1,
            30 | 62 | 63 | 64 | 65 | 66 | 67 | 68 | 69 | 80 | 97 => 2,
            x if x == K_TPL || x == K_APPLY_TPL => 2,
            0 | -1 => 3,
            _ => 0,
        };
        // Keep the dynamic gain values; other original static labels are replaced
        // by page-owned headings and helpers, without changing action controls.
        if id == 0 || id == -1 {
            let mut b = [0u16; 160];
            let n = GetWindowTextW(c.h as HWND, b.as_mut_ptr(), 160);
            let text = String::from_utf16_lossy(&b[..n as usize]);
            if text.ends_with(" dB") {
                c.category = 0;
                c.x = 748;
                c.y = if c.y < 420 { 1 } else { 2 };
            }
        }
        c.w = 808;
        c.z = 36;
    }
    for (i, title) in ["Capture", "Appearance", "Files / app"].iter().enumerate() {
        let b = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(title).as_ptr(),
            WS_CHILD | BS_OWNERDRAW as u32 | WS_TABSTOP,
            24 + i as i32 * 160,
            90,
            152,
            30,
            parent,
            (82 + i) as _,
            inst,
            std::ptr::null(),
        );
        SendMessageW(b, WM_SETFONT, font as usize, 0);
        crate::ui::interactive(b);
        SetPropW(b, wide("settings-nav").as_ptr(), 1usize as _);
    }
    for i in 0..19 {
        let b = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | WS_TABSTOP | BS_OWNERDRAW as u32,
            0,
            0,
            1,
            1,
            h,
            (300 + i) as _,
            inst,
            std::ptr::null(),
        );
        let names = [
            "Charcoal",
            "Midnight",
            "Plum",
            "Ember",
            "Forest",
            "High contrast",
            "Mint",
            "Sky",
            "Lilac",
            "Amber",
            "Ice",
            "Peach",
            "Rose",
            "Silver",
            "Balanced split",
            "Compact capture",
            "Wide library",
            "Stacked studio",
            "Sidebar",
        ];
        SetWindowTextW(b, wide(names[i as usize]).as_ptr());
        SendMessageW(b, WM_SETFONT, font as usize, 0);
        SetWindowSubclass(b, Some(child_proc), 77, 0);
        crate::ui::interactive(b);
    }
    SetPropW(parent, wide("settings-viewport").as_ptr(), h as _);
    *state().lock().unwrap() = s;
}
pub unsafe fn layout(parent: HWND, shown: bool) {
    let mut s = state().lock().unwrap().clone();
    if s.view == 0 {
        return;
    }
    let h = s.view as HWND;
    ShowWindow(h, if shown { SW_SHOW } else { SW_HIDE });
    ShowWindow(s.bar as HWND, if shown { SW_SHOW } else { SW_HIDE });
    for id in 82..=84 {
        ShowWindow(
            GetDlgItem(parent, id),
            if shown { SW_SHOW } else { SW_HIDE },
        );
    }
    // Library density buttons live on Replay only. The new "Larger" split
    // control has to follow the older "Compact" button in/out of Settings.
    for id in [36, 90] {
        let c = GetDlgItem(parent, id);
        if !c.is_null() {
            ShowWindow(c, if shown { SW_HIDE } else { SW_SHOW });
        }
    }
    if !shown {
        for key in ["result", "footer"] {
            ShowWindow(
                GetPropW(parent, wide(&format!("slot-{key}")).as_ptr()) as HWND,
                SW_SHOW,
            );
        }
        for key in ["heading", "tagline"] {
            ShowWindow(
                GetPropW(parent, wide(&format!("slot-{key}")).as_ptr()) as HWND,
                SW_SHOW,
            );
        }
        return;
    }
    let mut r = RECT::default();
    GetClientRect(parent, &mut r);
    s.height = (r.bottom - 110).max(150);
    s.mode = GetPropW(parent, wide("display-layout").as_ptr()) as usize;
    let sidebar = if r.right < 720 { 160 } else { 208 };
    s.width = (r.right - sidebar - 48).max(300);
    s.total = page_total(s.category, s.width);
    *state().lock().unwrap() = s.clone();
    crate::display::put(h, sidebar + 24, 94, s.width, s.height);
    crate::display::put(s.bar as HWND, r.right - 18, 94, 6, s.height);
    for id in 82..=84 {
        crate::display::put(
            GetDlgItem(parent, id),
            20,
            110 + (id - 82) * 58,
            sidebar - 20,
            46,
        );
    }
    for key in ["result", "footer"] {
        ShowWindow(
            GetPropW(parent, wide(&format!("slot-{key}")).as_ptr()) as HWND,
            SW_HIDE,
        );
    }
    // Keep service actions in the sidebar, not across the settings content.
    crate::display::put(GetDlgItem(parent, 4), 20, r.bottom - 100, sidebar - 36, 38);
    crate::display::put(GetDlgItem(parent, 5), 20, r.bottom - 54, sidebar - 36, 38);
    crate::display::put(GetDlgItem(parent, 14), 24, 22, 170, 40);
    crate::display::put(GetDlgItem(parent, 89), r.right - 292, 22, 130, 38);
    for key in ["heading", "tagline"] {
        ShowWindow(
            GetPropW(parent, wide(&format!("slot-{key}")).as_ptr()) as HWND,
            SW_HIDE,
        );
    }
    for key in ["rightheading", "righthint"] {
        ShowWindow(
            GetPropW(parent, wide(&format!("slot-{key}")).as_ptr()) as HWND,
            SW_HIDE,
        );
    }
    drop(s);
    scroll(0);
}
/// Files / app lower page: (status_y, file_names_y). The amber status line and the
/// resolve button only take room when they are actually shown.
fn files_tail() -> (i32, i32) {
    let mut y = 1220;
    if IMPORT_STATE.load(std::sync::atomic::Ordering::Relaxed) == 2 {
        y = 1264;
    }
    let status_y = y;
    if !backup_status().lock().unwrap().is_empty() {
        y += 56;
    }
    (status_y, y + 8)
}
/// The narrow layout puts Apply template under the box instead of beside it.
fn tpl_extra(width: i32) -> i32 {
    if width < 530 { 44 } else { 0 }
}
fn page_total(category: usize, width: i32) -> i32 {
    match category {
        0 => {
            if width < 530 {
                1352
            } else {
                996
            }
        }
        1 => appearance_geometry(width).2 + 350,
        2 => files_tail().1 + 298 + tpl_extra(width) + 16,
        _ => 100,
    }
}
fn control_rect(c: &Control, _mode: usize, width: i32) -> (i32, i32, i32, i32) {
    let id = c.id;
    let narrow = width < 530;
    if c.category == 1 {
        let end = appearance_geometry(width).2;
        return match id {
            46 => (width / 2, end + 30, width / 2 - 24, 38),
            95 => (width / 2, end + 94, width / 2 - 24 - 140, 38),
            41 => (width / 2 + width / 2 - 24 - 132, end + 94, 132, 38),
            x if x == crate::ctl_ids::TAGLINE_EDIT as i32 => (width / 2, end + 158, width / 2 - 24, 38),
            91 => (width / 2, end + 222, width / 2 - 24, 38),
            92 => (width / 2, end + 286, width / 2 - 24, 38),
            _ => (0, 0, 0, 0),
        };
    }
    if c.category == 2 {
        let row = match id {
            80 => 0,
            62 => 1,
            30 => 2,
            63 => 3,
            64 => 4,
            65 => 5,
            66 => 6,
            67 | 68 => {
                let half = (width - 48) / 2;
                return (if id == 67 { 20 } else { 28 + half }, 1172, half, 38);
            }
            69 => return (20, 1218, width - 40, 38),
            97 => return (20, files_tail().1 + 78, width - 40, 38),
            x if x == K_TPL => {
                let w = if narrow { width - 40 } else { width - 40 - 138 };
                return (20, files_tail().1 + 152, w, 36);
            }
            x if x == K_APPLY_TPL => {
                return if narrow {
                    (20, files_tail().1 + 196, width - 40, 38)
                } else {
                    (width - 20 - 130, files_tail().1 + 151, 130, 38)
                };
            }
            _ => 3,
        };
        return (20, 160 + row * 120, width - 40, 38);
    }
    let row = match id {
        20 => 0,
        23 => 1,
        21 => 2,
        42 => 3,
        x if x == crate::ctl_ids::REPLAY_SLIDER as i32 || crate::ctl_ids::QUICK_LEN.iter().any(|q| *q as i32 == x) => 3,
        7 => 4,
        12 => 5,
        26 => 6,
        27 => 7,
        73 => 8,
        79 => 9,
        33 | 32 => 10,
        34 | 35 => 11,
        x if x == K_PAUSE || x == K_APPLY_PAUSE => 12,
        _ => {
            if c.y == 1 {
                6
            } else {
                7
            }
        }
    };
    let step = if narrow { 92 } else { 66 };
    let y = 112 + row * step;
    let quick = crate::ctl_ids::QUICK_LEN.iter().position(|q| *q as i32 == id);
    if id == 42 || id == crate::ctl_ids::REPLAY_SLIDER as i32 || quick.is_some() {
        // Replay length row: slider and value button, then the quick lengths.
        let (x0, w0) = if narrow { (20, width - 40) } else { (width * 43 / 100, width - width * 43 / 100 - 20) };
        // Two lines in both layouts: slider with the value button, then the four quick lengths.
        let (val, gap) = (64, 6);
        let pill = (w0 - 3 * gap) / 4;
        let (y1, y2, h1, h2) = if narrow { (y + 30, y + 64, 30, 24) } else { (y + 4, y + 36, 28, 24) };
        return match (id, quick) {
            (42, _) => (x0 + w0 - val, y1, val, h1),
            (_, Some(i)) => (x0 + i as i32 * (pill + gap), y2, pill, h2),
            _ => (x0, y1, w0 - val - gap, h1),
        };
    }
    let x = if narrow { 20 } else { width * 43 / 100 };
    let w = width - x - 20;
    if id == 26 || id == 27 {
        return (x, y + if narrow { 34 } else { 8 }, w - 70, 32);
    }
    if id == 0 || id == -1 {
        return (width - 80, y + if narrow { 36 } else { 10 }, 60, 28);
    }
    if id == 33 || id == 34 || id == K_PAUSE {
        return (x, y + if narrow { 34 } else { 8 }, (w - 8) / 2, 36);
    }
    if id == 32 || id == 35 || id == K_APPLY_PAUSE {
        return (
            x + (w + 8) / 2,
            y + if narrow { 34 } else { 8 },
            (w - 8) / 2,
            36,
        );
    }
    (x, y + if narrow { 34 } else { 8 }, w, 36)
}
pub unsafe fn jump(section: usize) {
    {
        let mut s = state().lock().unwrap();
        s.category = section.min(2);
        s.offset = 0;
        s.total = page_total(s.category, s.width);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            let category = state().lock().unwrap().category;
            let _ = std::fs::write(root.join("settings-page.txt"), category.to_string());
        }
    }
    scroll(0);
}
pub unsafe fn scroll(delta: i32) {
    // Window movement and paint can synchronously re-enter focus/paint handlers.
    // Keep only the state update under the lock, never a Win32 call.
    let s = {
        let mut s = state().lock().unwrap();
        s.total = page_total(s.category, s.width);
        s.offset = (s.offset + delta).clamp(0, (s.total - s.height).max(0));
        s.clone()
    };
    let h = s.view as HWND;
    if h.is_null() {
        return;
    }
    let active = s.category;
    let parent = GetParent(h);
    if crate::ui::motion::pill_set(active, crate::ui::motion::allowed(h)) {
        SetTimer(h, crate::ui::motion::PILL_TIMER, 16, None);
    }
    for i in 0..3 {
        let b = GetDlgItem(parent, 82 + i);
        SetPropW(
            b,
            wide("section-active").as_ptr(),
            if i as usize == active {
                1usize as _
            } else {
                std::ptr::null_mut()
            },
        );
        InvalidateRect(b, std::ptr::null(), 0);
    }
    // The header pill belongs to the Appearance page only.
    InvalidateRect(parent, std::ptr::null(), 0);
    for c in &s.controls {
        if GetDlgCtrlID(c.h as HWND) == 63 {
            // Show the stored state every time the page is shown; the editor checkbox can change it.
            let on = !crate::file_prefs::delete_confirm_skipped(&crate::file_prefs::prefs_file());
            SetWindowTextW(
                c.h as HWND,
                wide(if on { "Confirm before deleting ON" } else { "Confirm before deleting OFF" }).as_ptr(),
            );
        }
        if GetDlgCtrlID(c.h as HWND) == 64 {
            SetWindowTextW(c.h as HWND, wide(&crate::file_prefs::share_label(crate::file_prefs::prefs_file().parent())).as_ptr());
        }
        ShowWindow(
            c.h as HWND,
            if c.category == s.category && !(s.category == 1 && [31, 22, 45].contains(&c.id)) {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
        if c.category == s.category {
            let (x, y, w, z) = control_rect(c, s.mode, s.width);
            crate::display::put(c.h as HWND, x, y - s.offset, w, z);
        }
    }
    for i in 0..19 {
        let b = GetDlgItem(h, 300 + i);
        ShowWindow(b, if active == 1 { SW_SHOW } else { SW_HIDE });
        if active == 1 {
            let (combo, n) = if i < 6 {
                (31, i)
            } else if i < 14 {
                (22, i - 6)
            } else {
                (45, i - 14)
            };
            SendMessageW(
                b,
                0x00f1,
                (SendMessageW(GetDlgItem(h, combo), CB_GETCURSEL, 0, 0) == n as isize) as usize,
                0,
            );
            let r = choice_rect(i, s.width);
            crate::display::put(
                b,
                r.left,
                r.top - s.offset,
                r.right - r.left,
                r.bottom - r.top,
            );
        }
    }
    let mut order: Vec<_> = s
        .controls
        .iter()
        .filter(|c| c.category == s.category && c.id > 0)
        .collect();
    order.sort_by_key(|c| {
        let (x, y, _, _) = control_rect(c, s.mode, s.width);
        (y, x)
    });
    let mut previous = HWND_TOP;
    for c in order {
        SetWindowPos(
            c.h as HWND,
            previous,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        previous = c.h as HWND;
    }
    RedrawWindow(
        h,
        std::ptr::null(),
        std::ptr::null_mut(),
        RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_NOERASE,
    );
    ShowWindow(
        s.bar as HWND,
        if s.total > s.height { SW_SHOW } else { SW_HIDE },
    );
    InvalidateRect(s.bar as HWND, std::ptr::null(), 0);
}
unsafe fn ensure(c: HWND) {
    let mut r = RECT::default();
    GetWindowRect(c, &mut r);
    let mut p = POINT {
        x: r.left,
        y: r.top,
    };
    ScreenToClient(GetParent(c), &mut p);
    let height = state().lock().unwrap().height;
    let delta = if p.y < 48 {
        p.y - 48
    } else if p.y + 42 > height - 12 {
        p.y + 54 - height
    } else {
        0
    };
    if delta != 0 {
        scroll(delta);
    }
}

unsafe extern "system" fn bar_proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    match m {
        WM_ERASEBKGND => 1,
        WM_MOUSEWHEEL => {
            scroll(if (w >> 16) as i16 > 0 { -96 } else { 96 });
            0
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP
            if m == WM_LBUTTONDOWN
                || windows_sys::Win32::UI::Input::KeyboardAndMouse::GetCapture() == h =>
        {
            if m == WM_LBUTTONDOWN {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetCapture(h);
            }
            let s = state().lock().unwrap().clone();
            let thumb = (r.bottom * s.height / s.total.max(1)).max(28);
            let y = ((l >> 16) as i16 as i32 - thumb / 2).clamp(0, (r.bottom - thumb).max(1));
            let value = y * (s.total - s.height) / (r.bottom - thumb).max(1);
            let delta = value - s.offset;
            drop(s);
            scroll(delta);
            if m == WM_LBUTTONUP {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            0
        }
        WM_PAINT => {
            let s = state().lock().unwrap().clone();
            let mut ps = PAINTSTRUCT::default();
            let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
            FillRect(dc, &r, crate::ui::style::brush(crate::ui::bg()));
            crate::ui::rounded(dc, r, crate::ui::panel(), crate::ui::panel(), 10);
            let th = (r.bottom * s.height / s.total.max(1)).max(28);
            let y = (r.bottom - th) * s.offset / (s.total - s.height).max(1);
            crate::ui::rounded(
                dc,
                RECT {
                    left: 1,
                    top: y,
                    right: r.right - 1,
                    bottom: y + th,
                },
                crate::ui::accent(),
                crate::ui::accent(),
                8,
            );
            crate::ui::end_buffer(h, &ps, dc, b);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
use crate::ui::style;
unsafe fn page_text(dc: HDC, value: &str, x: i32, y: i32, w: i32, px: i32, bold: bool, color: u32) {
    style::label(
        dc,
        value,
        RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + if px >= 20 { 32 } else { 24 },
        },
        style::font(
            &crate::display::font_name(),
            px,
            if bold { 600 } else { 400 },
        ),
        color,
        DT_SINGLELINE | DT_END_ELLIPSIS | DT_VCENTER,
    );
}
/// Hint text that wraps to `lines` lines instead of cutting off with an ellipsis.
unsafe fn page_wrapped(dc: HDC, value: &str, x: i32, y: i32, w: i32, px: i32, lines: i32, color: u32) {
    style::label(
        dc,
        value,
        RECT { left: x, top: y, right: x + w - 24, bottom: y + lines * (px + 4) },
        style::font(&crate::display::font_name(), px, 400),
        color,
        DT_WORDBREAK | DT_END_ELLIPSIS,
    );
}
unsafe fn paint_page(_h: HWND, dc: HDC, r: RECT) {
    let s = state().lock().unwrap().clone();
    let t = crate::ui::style_theme();
    let o = s.offset;
    let w = r.right;
    let title = ["Capture", "Appearance", "Files / app"][s.category.min(2)];
    let subtitle = [
        "Changes apply right away.",
        "Theme, accent and layout.",
        "Existing clips stay put. No saved videos change.",
    ][s.category.min(2)];
    page_text(dc, title, 20, 10 - o, w - 40, 24, true, t.text);
    page_text(dc, subtitle, 20, 46 - o, w - 40, 13, false, t.muted);
    if s.category == 0 {
        page_text(
            dc,
            "Capture changes clear the unsaved replay, not saved clips.",
            20,
            78 - o,
            w - 40,
            12,
            false,
            t.amber,
        );
        let rows = [
            ("Resolution", "Output width"),
            ("Capture FPS", "Frames per second"),
            ("Capture path", "Video conversion"),
            ("Replay length", "5 to 300 seconds"),
            ("System audio", "Separate System track"),
            ("Microphone", "Separate Mic track"),
            ("System gain", "Live gain, no restart"),
            ("Microphone gain", "Live gain, no restart"),
            ("Voice chat apps", "Separate Comms track"),
            ("Audio devices", "System output and mic input"),
            ("Save shortcut", "Edit, then Apply"),
            ("Save feedback", "Sound and notification"),
            ("Pause shortcut", "Edit, then Apply"),
        ];
        let step = if w < 530 { 92 } else { 66 };
        for (i, (label, helper)) in rows.iter().enumerate() {
            let y = 112 + i as i32 * step - o;
            style::rect(
                dc,
                RECT {
                    left: 20,
                    top: y,
                    right: w - 20,
                    bottom: y + 1,
                },
                t.line,
                t.line,
                0,
            );
            let lw = if w < 530 { w - 40 } else { w * 43 / 100 - 24 };
            page_text(dc, label, 20, y + 5, lw, 14, true, t.text);
            if w >= 530 {
                page_text(dc, helper, 20, y + 28, lw, 12, false, t.muted);
            }
            let slot = if i == 10 { Some(0) } else if i == 12 { Some(1) } else { None };
            if let Some(slot) = slot {
                let (m, bad) = shortcut_msgs().lock().unwrap()[slot].clone();
                if !m.is_empty() {
                    let (x, my) = if w < 530 { (20, y + 72) } else { (w * 43 / 100, y + 48) };
                    page_wrapped(dc, &m, x, my, w - x - 20, 12, 1, if bad { t.amber } else { t.muted });
                }
            }
        }
    } else if s.category == 1 {
        let (accent, layout, end) = appearance_geometry(w);
        for (top, bottom, label) in [
            (82, accent - 18, "THEME"),
            (accent, layout - 18, "ACCENT"),
            (layout, end, "LAYOUT"),
        ] {
            style::rect(
                dc,
                RECT {
                    left: 20,
                    top: top - o,
                    right: w - 20,
                    bottom: bottom - o,
                },
                t.panel,
                t.border,
                16,
            );
            page_text(dc, label, 40, top + 14 - o, w - 80, 12, true, t.muted);
        }
        page_text(
            dc,
            "Font family",
            24,
            end + 34 - o,
            w / 2 - 30,
            14,
            true,
            t.text,
        );
        page_text(
            dc,
            "App identity",
            24,
            end + 98 - o,
            w / 2 - 30,
            14,
            true,
            t.text,
        );
        page_text(dc, "Tagline", 24, end + 162 - o, w / 2 - 30, 14, true, t.text);
        page_text(dc, "Animations", 24, end + 226 - o, w / 2 - 30, 14, true, t.text);
        page_text(dc, "Custom colours", 24, end + 290 - o, w / 2 - 30, 14, true, t.text);
    } else {
        for (i, (a, b)) in [
            ("Save folder", "Existing files are never moved"),
            ("Start with Windows", "Uses this portable app location"),
            ("Updates", "Check, download and verify before installation"),
            ("Delete confirmation", "Ask before moving an open clip to Trash. Clips stay recoverable there."),
            ("Share size default", "Default export target, not a guaranteed file size. Only preselects the editor size."),
            ("Setup guide", "Shows the first-run guide on the next start. Current settings stay until you finish it."),
            ("Close button", "Hide keeps recording in the tray. Exit closes ZimClip and stops recording. With no tray icon, closing exits."),
        ]
        .iter()
        .enumerate()
        {
            let y = 82 + i as i32 * 120 - o;
            page_text(dc, a, 20, y, w - 40, 14, true, t.text);
            page_wrapped(dc, b, 20, y + 22, w - 40, 12, 3, t.muted);
        }
        let y = 922 - o;
        page_text(dc, "Shortcuts", 20, y, w - 40, 14, true, t.text);
        let shortcuts_text = format!("Global: Save on Home (Alt+G by default). {} pauses or resumes and clears the unsaved replay. Alt+O opens Clips. Alt+Q quits.", pause_label().lock().unwrap().clone());
        page_wrapped(dc, &shortcuts_text, 20, y + 22, w - 40, 12, 3, t.muted);
        page_wrapped(dc, "Editor: Space plays or pauses when the timeline or Play button has focus. Left and Right step 33 ms, with Shift one second. [ and ] set the trim start and end when the timeline has focus.", 20, y + 74, w - 56, 12, 4, t.muted);
        let y = files_tail().1 - o;
        page_text(dc, "File names", 20, y, w - 40, 14, true, t.text);
        page_wrapped(dc, "Applies to new saved replays only.", 20, y + 22, w - 40, 12, 1, t.muted);
        page_wrapped(dc, "Existing clips are not renamed.", 20, y + 38, w - 40, 12, 1, t.muted);
        let ex = name_example().lock().unwrap().clone();
        page_wrapped(dc, &format!("Example: {ex}"), 20, y + 54, w - 40, 12, 1, t.muted);
        let ex2 = tpl_extra(w);
        page_text(dc, "Custom template", 20, y + 130, w - 40, 14, true, t.text);
        let (preview, problem, is_err) = tpl_feedback().lock().unwrap().clone();
        let py = y + 196 + ex2;
        if !preview.is_empty() {
            page_wrapped(dc, &format!("Preview: {preview}"), 20, py, w - 40, 12, 1, t.muted);
        }
        if !problem.is_empty() {
            page_wrapped(dc, &problem, 20, py + 18, w - 40, 12, 2, if is_err { t.amber } else { t.muted });
        }
        page_wrapped(dc, "Tokens: {date} 2026-11-27, {time} 143005, {app} the app name set in Settings. Letters, digits, - and _ only. The game name is not available.", 20, py + 54, w - 40, 12, 3, t.muted);
        let status = backup_status().lock().unwrap().clone();
        if !status.is_empty() {
            page_wrapped(dc, &status, 20, files_tail().0 - o, w - 40, 12, 3, t.amber);
        }
        let y = 1072 - o;
        page_text(dc, "Settings backup", 20, y, w - 40, 14, true, t.text);
        page_wrapped(dc, if IMPORT_STATE.load(std::sync::atomic::Ordering::Relaxed) == 1 { "Settings were imported. This page is locked and may show old values until you reopen ZimClip. Export reads the saved values." } else if IMPORT_STATE.load(std::sync::atomic::Ordering::Relaxed) == 2 { "Import is off: an earlier import did not finish and could not be undone automatically. Some settings may be partly changed, a mix of old and new values. Use Turn imports back on, below." } else { "Backup covers capture, audio, layout, appearance and file-name style. Not covered: custom colours, fonts, folders, devices, shortcuts, name template." }, 20, y + 22, w - 40, 12, 4, t.muted);
    }
}
unsafe fn preview_click(_h: HWND, _l: isize) {}

fn appearance_geometry(w: i32) -> (i32, i32, i32) {
    let rows = if w < 600 { 3 } else { 2 };
    let accent = 82 + 52 + rows * 112 + 18;
    let layout = accent + 100 + 18;
    // Five layout cards: one row of five when wide, three rows of two when narrow.
    let end = layout + 52 + if w < 600 { 306 } else { 102 } + 18;
    (accent, layout, end)
}
fn choice_rect(i: i32, w: i32) -> RECT {
    let (a, l, _) = appearance_geometry(w);
    let (x, y, cw, ch) = if i < 6 {
        let cols = if w < 600 { 2 } else { 3 };
        let cw = (w - 80 - (cols - 1) * 12) / cols;
        (40 + (i % cols) * (cw + 12), 134 + (i / cols) * 112, cw, 100)
    } else if i < 14 {
        let step = ((w - 80) / 8).min(44);
        (40 + (i - 6) * step, a + 44, 36, 36)
    } else {
        let cols = if w < 600 { 2 } else { 5 };
        let cw = (w - 80 - (cols - 1) * 12) / cols;
        let n = i - 14;
        (
            40 + (n % cols) * (cw + 12),
            l + 46 + (n / cols) * 102,
            cw,
            90,
        )
    };
    RECT {
        left: x,
        top: y,
        right: x + cw,
        bottom: y + ch,
    }
}
unsafe fn choose_index(h: HWND, id: i32) {
    let n = id - 300;
    let (combo, index) = if n < 6 {
        (31, n)
    } else if n < 14 {
        (22, n - 6)
    } else {
        (45, n - 14)
    };
    let c = GetDlgItem(h, combo);
    SendMessageW(c, CB_SETCURSEL, index as usize, 0);
    SendMessageW(
        GetParent(h),
        WM_COMMAND,
        combo as usize | ((CBN_SELCHANGE as usize) << 16),
        c as isize,
    );
    scroll(0);
}
unsafe fn draw_choice(d: &DRAWITEMSTRUCT) {
    let id = d.CtlID as i32 - 300;
    let t = crate::ui::style_theme();
    {
        let b = CreateSolidBrush(crate::ui::backdrop(d.hwndItem, d.rcItem));
        FillRect(d.hDC, &d.rcItem, b);
        DeleteObject(b);
    }
    let parent = GetParent(d.hwndItem);
    let (combo, index) = if id < 6 {
        (31, id)
    } else if id < 14 {
        (22, id - 6)
    } else {
        (45, id - 14)
    };
    let selected = SendMessageW(GetDlgItem(parent, combo), CB_GETCURSEL, 0, 0) == index as isize;
    let mut r = d.rcItem;
    let w = r.right;
    let height = r.bottom;
    FillRect(d.hDC, &r, style::brush(t.panel));
    let focus = d.itemState & ODS_FOCUS != 0;
    if id < 6 {
        let names = [
            "Charcoal",
            "Midnight",
            "Plum",
            "Ember",
            "Forest",
            "High contrast",
        ];
        let desc = [
            "Neutral dark",
            "Cool blue",
            "Soft purple",
            "Warm dark",
            "Calm green",
            "Maximum legibility",
        ];
        style::rect(
            d.hDC,
            r,
            t.raise,
            if selected || focus {
                t.accent
            } else {
                t.border
            },
            12,
        );
        let c = crate::ui::theme_colors(id as usize);
        let bg = if id == 5 { 0xeeeeee } else { c[1] };
        let saved = SaveDC(d.hDC);
        let clip = CreateRoundRectRgn(8, 8, w - 8, 46, 16, 16);
        ExtSelectClipRgn(d.hDC, clip, RGN_AND);
        DeleteObject(clip);
        let start = if id == 5 { 0 } else { style::mix(bg, 0, 55) };
        let end = if id == 5 {
            0xffffff
        } else {
            style::mix(bg, c[2], 35)
        };
        for x in 0..(w - 16) {
            let color = style::mix(start, end, (x * 100 / (w - 17)).min(100) as u32);
            let b = CreateSolidBrush(color);
            FillRect(
                d.hDC,
                &RECT {
                    left: 8 + x,
                    top: 8,
                    right: 9 + x,
                    bottom: 46,
                },
                b,
            );
            DeleteObject(b);
        }
        RestoreDC(d.hDC, saved);
        page_text(d.hDC, names[id as usize], 10, 54, w - 20, 14, true, t.text);
        page_text(d.hDC, desc[id as usize], 10, 76, w - 20, 12, false, t.muted);
    } else if id < 14 {
        let color = crate::ui::accent_colors()[index as usize];
        let ring = if selected || focus { t.text } else { t.panel };
        style::disc(d.hDC, RECT { left: 1, top: 1, right: 35, bottom: 35 }, ring, None);
        style::disc(d.hDC, RECT { left: 3, top: 3, right: 33, bottom: 33 }, t.panel, None);
        style::disc(d.hDC, RECT { left: 5, top: 5, right: 31, bottom: 31 }, color, None);
    } else {
        style::rect(
            d.hDC,
            r,
            t.raise,
            if selected || focus {
                t.accent
            } else {
                t.border
            },
            12,
        );
        let parts = match index {
            1 => vec![(8, 8, w - 8, 54)],
            2 => vec![(8, 8, w / 3, 54), (w / 3 + 4, 8, w - 8, 54)],
            3 => vec![(8, 8, w - 8, 28), (8, 32, w - 8, 54)],
            4 => vec![(8, 8, w / 4, 54), (w / 4 + 4, 8, w - 8, 54)],
            _ => vec![(8, 8, w / 2 - 2, 54), (w / 2 + 2, 8, w - 8, 54)],
        };
        for (a, b, c, e) in parts {
            style::rect(
                d.hDC,
                RECT {
                    left: a,
                    top: b,
                    right: c,
                    bottom: e,
                },
                t.bg,
                t.border,
                6,
            );
        }
        page_text(
            d.hDC,
            ["Balanced", "Compact", "Wide", "Stacked", "Sidebar"][index as usize],
            8,
            height - 28,
            w - 16,
            13,
            selected,
            t.text,
        );
    }
    if focus {
        r.left += 2;
        r.top += 2;
        r.right -= 2;
        r.bottom -= 2;
        style::focus(d.hDC, r, t);
    }
}
