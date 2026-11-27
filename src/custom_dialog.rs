//! Custom theme editor, shared by Settings and setup. The window uses fixed system colours, so it
//! stays readable whatever palette the user picks. Nothing is written here: the caller decides when
//! colours and presets reach disk (Settings right away, setup only on Done).
use crate::custom_theme::{self as ct, Custom, Preset};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::{Dialogs::*, DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_SELECTED}, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};

#[derive(Clone)]
pub struct Edit {
    pub custom: Custom,
    pub accent: usize,
    pub presets: Vec<Preset>,
}
pub enum Outcome {
    /// Keep these colours as the Custom theme (and the possibly changed preset list).
    Apply(Edit),
    /// Go back to a built-in theme; the preset list may still have changed.
    Reset(Vec<Preset>),
    Cancel,
}

struct State<'a> {
    edit: Edit,
    preview: &'a dyn Fn(Custom, usize),
    done: Option<u8>, // 1 apply, 2 cancel, 3 reset
    font: HFONT,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn child(h: HWND, class: &str, text: &str, id: i32, style: u32, r: (i32, i32, i32, i32), font: HFONT) -> HWND {
    let c = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP * (class != "STATIC") as u32 | style,
        r.0,
        r.1,
        r.2,
        r.3,
        h,
        id as usize as _,
        GetModuleHandleW(std::ptr::null()),
        std::ptr::null(),
    );
    SendMessageW(c, WM_SETFONT, font as usize, 1);
    c
}
unsafe fn state<'a>(h: HWND) -> Option<&'a mut State<'a>> {
    let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut State;
    p.as_mut()
}
unsafe fn refill(h: HWND, st: &State, select: Option<usize>) {
    let list = GetDlgItem(h, 210);
    SendMessageW(list, LB_RESETCONTENT, 0, 0);
    for p in &st.edit.presets {
        SendMessageW(list, LB_ADDSTRING, 0, wide(&p.name).as_ptr() as isize);
    }
    if let Some(i) = select.or(if st.edit.presets.is_empty() { None } else { Some(0) }) {
        SendMessageW(list, LB_SETCURSEL, i, 0);
    }
    InvalidateRect(list, std::ptr::null(), 1);
    let any = !st.edit.presets.is_empty();
    EnableWindow(GetDlgItem(h, 211), any as i32);
    EnableWindow(GetDlgItem(h, 214), any as i32);
}
unsafe fn changed(h: HWND, st: &State) {
    (st.preview)(st.edit.custom, st.edit.accent);
    InvalidateRect(h, std::ptr::null(), 1);
}
unsafe fn pick(h: HWND, current: u32) -> Option<u32> {
    static mut CUSTOM: [u32; 16] = [0xffffff; 16];
    let mut cc: CHOOSECOLORW = std::mem::zeroed();
    cc.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
    cc.hwndOwner = h;
    cc.rgbResult = current;
    cc.lpCustColors = std::ptr::addr_of_mut!(CUSTOM) as *mut u32;
    cc.Flags = CC_RGBINIT | CC_FULLOPEN;
    (ChooseColorW(&mut cc) != 0).then_some(cc.rgbResult & 0xffffff)
}
unsafe fn text_of(h: HWND) -> String {
    let mut b = vec![0u16; (GetWindowTextLengthW(h) + 1) as usize];
    let n = GetWindowTextW(h, b.as_mut_ptr(), b.len() as i32);
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
/// Fixed dark look for this window (never the user's picked palette, so it stays readable).
fn fixed() -> crate::ui::style::Theme {
    use crate::ui::style::mix;
    let (bg, panel, text, accent) = (0x16120f, 0x211b16, 0xefece8, 0xc2e89b);
    crate::ui::style::Theme {
        bg,
        panel,
        raise: 0x2c251f,
        hover: 0x39312a,
        border: 0x3b322a,
        line: 0x2a231d,
        text,
        muted: 0xada298,
        dim: 0x7d746b,
        accent,
        ink: 0x121a0b,
        amber: 0x5ec0f5,
        rec: mix(0x4040e0, text, 10),
    }
}
fn hex(c: u32) -> String {
    format!("#{:02X}{:02X}{:02X}", c & 255, (c >> 8) & 255, (c >> 16) & 255)
}
unsafe fn ui_font(px: i32, weight: i32) -> HFONT {
    crate::ui::style::font(&crate::display::font_name(), px, weight)
}
unsafe fn fill(dc: HDC, r: RECT, c: u32) {
    let b = CreateSolidBrush(c);
    FillRect(dc, &r, b);
    DeleteObject(b as _);
}

unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_NCCREATE => {
            let cs = &*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(h, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(h, m, w, l)
        }
        WM_CLOSE => {
            if let Some(st) = state(h) {
                st.done = Some(2);
            }
            0
        }
        WM_ERASEBKGND => {
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            fill(w as HDC, r, fixed().bg);
            1
        }
        WM_MEASUREITEM => {
            let m = &mut *(l as *mut MEASUREITEMSTRUCT);
            if m.CtlID == 210 {
                m.itemHeight = 30;
                return 1;
            }
            0
        }
        WM_CTLCOLOREDIT => {
            let t = fixed();
            SetTextColor(w as HDC, t.text);
            SetBkColor(w as HDC, t.panel);
            static B: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
            *B.get_or_init(|| CreateSolidBrush(fixed().panel) as usize) as isize
        }
        0x0134 => {
            // WM_CTLCOLORLISTBOX: the space under the last preset stays dark.
            let t = fixed();
            SetTextColor(w as HDC, t.text);
            SetBkColor(w as HDC, t.panel);
            static B: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
            *B.get_or_init(|| CreateSolidBrush(fixed().panel) as usize) as isize
        }
        WM_DRAWITEM => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            let Some(st) = state(h) else { return 0 };
            let t = fixed();
            if d.CtlID == 210 {
                // Preset list row.
                fill(d.hDC, d.rcItem, t.panel);
                if d.itemID != u32::MAX {
                    let sel = d.itemState & ODS_SELECTED != 0;
                    let row = RECT { left: d.rcItem.left + 4, top: d.rcItem.top + 2, right: d.rcItem.right - 4, bottom: d.rcItem.bottom - 2 };
                    if sel {
                        crate::ui::style::rect(d.hDC, row, crate::ui::style::mix(t.panel, t.accent, 18), t.accent, 8);
                    }
                    if let Some(p) = st.edit.presets.get(d.itemID as usize) {
                        let sw = RECT { left: row.left + 8, top: row.top + 5, right: row.left + 24, bottom: row.bottom - 5 };
                        crate::ui::style::rect(d.hDC, sw, p.colors.bg, t.border, 4);
                        crate::ui::style::label(
                            d.hDC,
                            &p.name,
                            RECT { left: row.left + 34, top: row.top, right: row.right - 8, bottom: row.bottom },
                            ui_font(14, 400),
                            t.text,
                            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                        );
                    }
                }
                return 1;
            }
            let mut title = [0u16; 64];
            let n = GetWindowTextW(d.hwndItem, title.as_mut_ptr(), 64).max(0) as usize;
            let title = String::from_utf16_lossy(&title[..n]).replace('&', "");
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            let mut b = RECT::default();
            GetWindowRect(d.hwndItem, &mut b);
            let disabled = d.itemState & ODS_DISABLED != 0;
            let state = crate::ui::style::State {
                hover: !disabled && pt.x >= b.left && pt.x < b.right && pt.y >= b.top && pt.y < b.bottom,
                pressed: d.itemState & ODS_SELECTED != 0,
                focused: d.itemState & ODS_FOCUS != 0,
                disabled,
                ..Default::default()
            };
            fill(d.hDC, d.rcItem, t.bg);
            if matches!(d.CtlID, 201 | 202 | 203) {
                // Colour row: swatch, name, hex, and a Change hint.
                let c = match d.CtlID { 201 => st.edit.custom.bg, 202 => st.edit.custom.panel, _ => st.edit.custom.text };
                let r = d.rcItem;
                let edge = if state.focused { t.accent } else if state.hover { t.muted } else { t.border };
                crate::ui::style::rect(d.hDC, r, if state.hover { t.hover } else { t.raise }, edge, 12);
                let sw = RECT { left: r.left + 10, top: r.top + 7, right: r.left + 38, bottom: r.bottom - 7 };
                crate::ui::style::rect(d.hDC, sw, c, t.muted, 8);
                crate::ui::style::label(d.hDC, &title, RECT { left: r.left + 50, top: r.top, right: r.left + 220, bottom: r.bottom }, ui_font(14, 600), t.text, DT_SINGLELINE | DT_VCENTER);
                crate::ui::style::label(d.hDC, &hex(c), RECT { left: r.left + 220, top: r.top, right: r.right - 90, bottom: r.bottom }, ui_font(13, 400), t.muted, DT_SINGLELINE | DT_VCENTER);
                crate::ui::style::label(d.hDC, "Change...", RECT { left: r.right - 90, top: r.top, right: r.right - 12, bottom: r.bottom }, ui_font(13, 600), t.accent, DT_SINGLELINE | DT_VCENTER | DT_RIGHT);
                return 1;
            }
            let kind = match d.CtlID {
                1 | 213 => crate::ui::style::Button::Primary,
                _ => crate::ui::style::Button::Default,
            };
            crate::ui::style::button(d.hDC, d.rcItem, &title, kind, state, t, ui_font(14, 600));
            1
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(h, &mut ps);
            if let Some(st) = state(h) {
                let t = fixed();
                let pal = ct::palette(st.edit.custom);
                let accent = crate::ui::accent_colors()[st.edit.accent.min(7)];
                // Preview: a small mock of the app in the user's own colours.
                crate::ui::style::rect(dc, RECT { left: 12, top: 10, right: 468, bottom: 92 }, pal[0], t.border, 14);
                crate::ui::style::rect(dc, RECT { left: 22, top: 20, right: 262, bottom: 82 }, pal[1], pal[2], 10);
                crate::ui::style::label(dc, "Sample heading", RECT { left: 34, top: 26, right: 254, bottom: 46 }, ui_font(15, 700), pal[3], DT_SINGLELINE | DT_VCENTER);
                crate::ui::style::label(dc, "Secondary text sample", RECT { left: 34, top: 48, right: 254, bottom: 70 }, ui_font(13, 400), pal[4], DT_SINGLELINE | DT_VCENTER);
                crate::ui::style::rect(dc, RECT { left: 276, top: 20, right: 458, bottom: 48 }, pal[2], pal[2], 10);
                crate::ui::style::label(dc, "Border colour (derived)", RECT { left: 286, top: 20, right: 450, bottom: 48 }, ui_font(13, 400), pal[3], DT_SINGLELINE | DT_VCENTER);
                crate::ui::style::rect(dc, RECT { left: 276, top: 54, right: 366, bottom: 82 }, accent, accent, 15);
                crate::ui::style::label(dc, "Accent", RECT { left: 276, top: 54, right: 366, bottom: 82 }, ui_font(13, 700), pal[0], DT_SINGLELINE | DT_VCENTER | DT_CENTER);
                // Warning: fixed colours, never the picked palette.
                let w = ct::warning(st.edit.custom);
                let msg = w.clone().unwrap_or_else(|| "Text contrast looks comfortable.".into());
                crate::ui::style::label(dc, &msg, RECT { left: 14, top: 94, right: 466, bottom: 126 }, ui_font(13, 400), if w.is_some() { t.amber } else { t.muted }, DT_WORDBREAK);
                crate::ui::style::label(dc, "COLOURS", RECT { left: 14, top: 128, right: 300, bottom: 144 }, ui_font(12, 700), t.dim, DT_SINGLELINE);
                crate::ui::style::label(dc, "PRESETS (they also keep the accent colour)", RECT { left: 14, top: 262, right: 466, bottom: 278 }, ui_font(12, 700), t.dim, DT_SINGLELINE);
                crate::ui::style::rect(dc, RECT { left: 12, top: 280, right: 352, bottom: 352 }, t.panel, t.border, 12);
                crate::ui::style::label(dc, "SAVE THIS LOOK AS A PRESET (up to 24 characters)", RECT { left: 14, top: 358, right: 466, bottom: 374 }, ui_font(12, 700), t.dim, DT_SINGLELINE);
                crate::ui::style::rect(dc, RECT { left: 12, top: 376, right: 352, bottom: 410 }, t.panel, t.border, 10);
                fill(dc, RECT { left: 12, top: 418, right: 468, bottom: 419 }, t.line);
            }
            EndPaint(h, &ps);
            0
        }
        WM_COMMAND => {
            let mut id = (w & 0xffff) as i32;
            let Some(st) = state(h) else { return 0 };
            if id == 210 && (w >> 16) as u32 == LBN_DBLCLK {
                id = 211;
            } else if id == 210 {
                return 0;
            }
            match id {
                201 | 202 | 203 => {
                    let cur = match id {
                        201 => st.edit.custom.bg,
                        202 => st.edit.custom.panel,
                        _ => st.edit.custom.text,
                    };
                    if let Some(c) = pick(h, cur) {
                        match id {
                            201 => st.edit.custom.bg = c,
                            202 => st.edit.custom.panel = c,
                            _ => st.edit.custom.text = c,
                        }
                        changed(h, st);
                    }
                }
                211 => {
                    let i = SendMessageW(GetDlgItem(h, 210), LB_GETCURSEL, 0, 0);
                    if i >= 0 {
                        if let Some(p) = st.edit.presets.get(i as usize).cloned() {
                            st.edit.custom = p.colors;
                            st.edit.accent = p.accent;
                            changed(h, st);
                        }
                    }
                }
                213 => match ct::clean_name(&text_of(GetDlgItem(h, 212))) {
                    Err(e) => {
                        MessageBoxW(h, wide(&e).as_ptr(), wide("Preset not saved").as_ptr(), MB_OK | MB_ICONINFORMATION);
                    }
                    Ok(name) => {
                        let p = Preset { name: name.clone(), colors: st.edit.custom, accent: st.edit.accent };
                        match ct::upsert(&mut st.edit.presets, p) {
                            Err(e) => {
                                MessageBoxW(h, wide(&e).as_ptr(), wide("Preset not saved").as_ptr(), MB_OK | MB_ICONINFORMATION);
                            }
                            Ok(()) => {
                                let at = st.edit.presets.iter().position(|x| x.name == name);
                                refill(h, st, at);
                                let e = GetDlgItem(h, 212);
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(e);
                                SendMessageW(e, 0xB1, 0, -1); // EM_SETSEL all
                            }
                        }
                    }
                },
                214 => {
                    let i = SendMessageW(GetDlgItem(h, 210), LB_GETCURSEL, 0, 0);
                    if i >= 0 && (i as usize) < st.edit.presets.len() {
                        let name = st.edit.presets[i as usize].name.clone();
                        let q = format!("Delete the preset \"{name}\"?");
                        if MessageBoxW(h, wide(&q).as_ptr(), wide("Delete preset").as_ptr(), MB_YESNO | MB_ICONQUESTION) == IDYES {
                            st.edit.presets.remove(i as usize);
                            refill(h, st, None);
                        }
                    }
                }
                215 => st.done = Some(3),
                1 => st.done = Some(1),
                2 => st.done = Some(2),
                _ => {}
            }
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

/// Modal editor. `preview` is called after every change so the real window shows the result.
pub unsafe fn run(parent: HWND, font: HFONT, start: Edit, preview: &dyn Fn(Custom, usize)) -> Outcome {
    let cls = wide("ZimClipThemeDialog");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: GetModuleHandleW(std::ptr::null()),
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let mut st = State { edit: start, preview, done: None, font };
    let (cw, ch) = (480, 472);
    let mut r = RECT { left: 0, top: 0, right: cw, bottom: ch };
    AdjustWindowRect(&mut r, WS_CAPTION | WS_SYSMENU, 0);
    let mut pr = RECT::default();
    GetWindowRect(parent, &mut pr);
    let (ww, wh) = (r.right - r.left, r.bottom - r.top);
    let x = pr.left + ((pr.right - pr.left) - ww) / 2;
    let y = pr.top + ((pr.bottom - pr.top) - wh) / 2;
    // Keep the whole window (title bar and buttons) inside the work area.
    let mut work = RECT::default();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as _, 0);
    let x = x.min(work.right - ww).max(work.left);
    let y = y.min(work.bottom - wh).max(work.top);
    let h = CreateWindowExW(
        WS_EX_DLGMODALFRAME,
        cls.as_ptr(),
        wide("Custom theme").as_ptr(),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
        x.max(0),
        y.max(0),
        ww,
        wh,
        parent,
        std::ptr::null_mut(),
        GetModuleHandleW(std::ptr::null()),
        &mut st as *mut State as *const _,
    );
    if h.is_null() {
        return Outcome::Cancel;
    }
    let own = BS_OWNERDRAW as u32;
    child(h, "BUTTON", "&Background", 201, own, (12, 146, 456, 34), font);
    child(h, "BUTTON", "&Panels", 202, own, (12, 184, 456, 34), font);
    child(h, "BUTTON", "&Text", 203, own, (12, 222, 456, 34), font);
    let list = child(h, "LISTBOX", "", 210, (LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_NOINTEGRALHEIGHT) as u32 | WS_VSCROLL, (14, 284, 336, 64), font);
    let _ = list;
    child(h, "BUTTON", "A&pply", 211, own, (362, 280, 106, 34), font);
    child(h, "BUTTON", "&Delete", 214, own, (362, 318, 106, 34), font);
    let edit = child(h, "EDIT", "", 212, ES_AUTOHSCROLL as u32, (22, 383, 320, 20), font);
    SendMessageW(edit, 0xC5, ct::MAX_NAME, 0); // EM_SETLIMITTEXT
    child(h, "BUTTON", "&Save preset", 213, own, (362, 376, 106, 34), font);
    child(h, "BUTTON", "&Reset to built-in", 215, own, (12, 428, 170, 34), font);
    child(h, "BUTTON", "OK", 1, own | BS_DEFPUSHBUTTON as u32, (288, 428, 84, 34), font);
    child(h, "BUTTON", "Cancel", 2, own, (384, 428, 84, 34), font);
    for id in [201, 202, 203, 211, 214, 213, 215, 1, 2] {
        crate::ui::interactive(GetDlgItem(h, id));
    }
    crate::ui::titlebar(h);
    refill(h, &st, None);
    EnableWindow(parent, 0);
    ShowWindow(h, SW_SHOW);
    (st.preview)(st.edit.custom, st.edit.accent);
    let mut msg: MSG = std::mem::zeroed();
    while st.done.is_none() && GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
        if msg.message == WM_KEYDOWN && msg.wParam == 0x0d && GetDlgCtrlID(msg.hwnd) == 212 {
            // Enter in the name box saves the preset; it does not close the window.
            SendMessageW(h, WM_COMMAND, 213, 0);
            continue;
        }
        if IsDialogMessageW(h, &mut msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    EnableWindow(parent, 1);
    let done = st.done.unwrap_or(2);
    SetWindowLongPtrW(h, GWLP_USERDATA, 0);
    DestroyWindow(h);
    SetForegroundWindow(parent);
    match done {
        1 => Outcome::Apply(st.edit),
        3 => Outcome::Reset(st.edit.presets),
        _ => Outcome::Cancel,
    }
}
