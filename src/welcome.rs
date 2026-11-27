use std::path::Path;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::*,
    UI::{Controls::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use crate::welcome_draw::{self as draw, Align, Op};
/// Choices on the setup pages. Only written to disk when setup finishes with Done.
static LAYOUT: AtomicUsize = AtomicUsize::new(0);
static THEME: AtomicUsize = AtomicUsize::new(0);
static ACCENT: AtomicUsize = AtomicUsize::new(0);
static POS: AtomicUsize = AtomicUsize::new(0);
/// Chosen performance preset (usize::MAX = none matches).
static PRESET: AtomicUsize = AtomicUsize::new(usize::MAX);
static ICON: AtomicUsize = AtomicUsize::new(0);
/// Uploaded image, held in memory until Done. Nothing is written for it before then.
fn upload() -> &'static std::sync::Mutex<Option<Vec<u8>>> {
    static U: std::sync::OnceLock<std::sync::Mutex<Option<Vec<u8>>>> = std::sync::OnceLock::new();
    U.get_or_init(Default::default)
}
const HINT_ID: usize = 106;
const FOLDER_ID: usize = 470;
const SECS_ID: usize = 471;
const SHARE_FIRST: usize = 480;
const DEV_FIRST: usize = 460;
/// Setup choices for the Audio and Clips pages. Written only when setup finishes with Done.
static SHARE: AtomicUsize = AtomicUsize::new(0);
static FOLDER: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);
static FOLDER_CHANGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SECS: std::sync::Mutex<Option<u64>> = std::sync::Mutex::new(None);
static DEVICES: std::sync::Mutex<Vec<Vec<(String, String)>>> = std::sync::Mutex::new(Vec::new());
static DEV_PICK: std::sync::Mutex<[Option<String>; 2]> = std::sync::Mutex::new([None, None]);
const NAME_ID: usize = 105;

fn rc(r: draw::Rc, ox: i32, oy: i32) -> RECT {
    RECT { left: r.0 + ox, top: r.1 + oy, right: r.2 + ox, bottom: r.3 + oy }
}
/// Run a draw list with GDI.
unsafe fn exec(dc: HDC, ops: &[Op], ox: i32, oy: i32) {
    let font = crate::display::font_name();
    for op in ops {
        match op {
            Op::Rect { r, fill, stroke, radius } => {
                let r = rc(*r, ox, oy);
                if *radius > 0 && (fill.is_some() || stroke.is_some()) {
                    // Rounded shapes are anti-aliased on the CPU and alpha-blended.
                    let px = crate::aa::rrect((r.right - r.left) as usize, (r.bottom - r.top) as usize, *radius, *fill, *stroke);
                    blit(dc, r.left, r.top, &px);
                    continue;
                }
                let b = match fill {
                    Some(c) => crate::ui::style::brush(*c) as _,
                    None => GetStockObject(NULL_BRUSH),
                };
                let pc = stroke.or(*fill).unwrap_or(0);
                let p = crate::ui::style::pen(pc, 1);
                let (ob, op_) = (SelectObject(dc, b), SelectObject(dc, p as _));
                Rectangle(dc, r.left, r.top, r.right, r.bottom);
                SelectObject(dc, ob);
                SelectObject(dc, op_);
            }
            Op::Text { r, s, px, weight, color, align } => {
                SetBkMode(dc, TRANSPARENT as _);
                crate::ui::style::label(
                    dc,
                    s,
                    rc(*r, ox, oy),
                    crate::ui::style::font(&font, *px, *weight),
                    *color,
                    DT_SINGLELINE
                        | DT_VCENTER
                        | DT_END_ELLIPSIS
                        | match align {
                            Align::Left => DT_LEFT,
                            Align::Center => DT_CENTER,
                            Align::Right => DT_RIGHT,
                        },
                );
            }
            Op::Poly { x, y, size, pts, color } => {
                let px = crate::aa::poly(*size, pts, 2. * (*size as f32 / 24.).max(0.8), *color);
                blit(dc, ox + x - crate::aa::POLY_PAD, oy + y - crate::aa::POLY_PAD, &px);
            }
        }
    }
}
/// Alpha-blend a premultiplied BGRA image at (x, y).
unsafe fn blit(dc: HDC, x: i32, y: i32, p: &crate::aa::Px) {
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = p.w as i32;
    bi.bmiHeader.biHeight = -(p.h as i32);
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(std::ptr::null_mut(), &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if bmp.is_null() || bits.is_null() {
        return;
    }
    std::ptr::copy_nonoverlapping(p.data.as_ptr(), bits as *mut u8, p.data.len());
    let mem = CreateCompatibleDC(dc);
    let old = SelectObject(mem, bmp as _);
    let f = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
    AlphaBlend(dc, x, y, p.w as i32, p.h as i32, mem, 0, 0, p.w as i32, p.h as i32, f);
    SelectObject(mem, old);
    DeleteDC(mem);
    DeleteObject(bmp as _);
}
unsafe fn view(h: HWND, step: usize) -> draw::View {
    let pos = POS.load(Relaxed);
    let preset = PRESET.load(Relaxed);
    let mut themes = [[0u32; 5]; 7];
    for (i, t) in themes.iter_mut().enumerate() {
        *t = crate::ui::theme_colors(i);
    }
    let _ = h;
    draw::View {
        step,
        layout: LAYOUT.load(Relaxed),
        theme: THEME.load(Relaxed),
        accent: ACCENT.load(Relaxed),
        themes,
        accents: crate::ui::accent_colors(),
        mic: MIC.load(Relaxed),
        apps: String::new(),
        pos,
        preset: (preset < 3).then_some(preset),
        name: crate::identity::name(),
        icon: ICON.load(Relaxed),
        has_upload: upload().lock().unwrap().is_some(),
        summary: SUMMARY.lock().unwrap().clone(),
    }
}
/// Select card or accent on a choice page and show it right away.
unsafe fn pick(h: HWND, step: usize, i: usize) {
    if step == 1 {
        LAYOUT.store(i, Relaxed);
        InvalidateRect(h, std::ptr::null(), 0);
    } else if step == 2 {
        THEME.store(i, Relaxed);
        live(h, Some(i), None);
    }
}
/// Custom-theme presets edited during setup. They exist only in memory until Done writes them.
/// Short values for the summary page cards, filled when that page is built.
static SUMMARY: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static PRESETS: std::sync::Mutex<Vec<crate::custom_theme::Preset>> = std::sync::Mutex::new(Vec::new());
static CUSTOM_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Open the shared custom-theme editor over setup. Preview is live; nothing touches disk here.
unsafe fn custom_editor(h: HWND) {
    use crate::custom_dialog::{run, Edit, Outcome};
    let prev_theme = THEME.load(Relaxed);
    let prev_accent = ACCENT.load(Relaxed);
    let prev_custom = crate::custom_theme::current();
    let start = Edit { custom: prev_custom, accent: prev_accent, presets: PRESETS.lock().unwrap().clone() };
    let preview = |c: crate::custom_theme::Custom, a: usize| {
        crate::custom_theme::set_current(c);
        crate::ui::set_theme(crate::custom_theme::THEME_INDEX);
        crate::ui::set_accent(a);
        InvalidateRect(h, std::ptr::null(), 0);
        EnumChildWindows(h, Some(crate::ui::invalidate_child), 0);
    };
    let font = GetStockObject(DEFAULT_GUI_FONT) as _;
    match run(h, font, start, &preview) {
        Outcome::Apply(e) => {
            crate::custom_theme::set_current(e.custom);
            THEME.store(crate::custom_theme::THEME_INDEX, Relaxed);
            ACCENT.store(e.accent, Relaxed);
            *PRESETS.lock().unwrap() = e.presets;
            CUSTOM_DIRTY.store(true, Relaxed);
            live(h, Some(crate::custom_theme::THEME_INDEX), Some(e.accent));
        }
        Outcome::Reset(p) => {
            let back = if prev_theme < 6 { prev_theme } else { 0 };
            *PRESETS.lock().unwrap() = p;
            CUSTOM_DIRTY.store(true, Relaxed);
            THEME.store(back, Relaxed);
            ACCENT.store(prev_accent, Relaxed);
            live(h, Some(back), Some(prev_accent));
        }
        Outcome::Cancel => {
            crate::custom_theme::set_current(prev_custom);
            THEME.store(prev_theme, Relaxed);
            ACCENT.store(prev_accent, Relaxed);
            live(h, Some(prev_theme), Some(prev_accent));
        }
    }
}
/// Microphone switch state on the audio page.
static MIC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const MIC_ID: usize = 104;
const CHIP_FIRST: usize = 130;

/// Apply a theme and/or accent to the whole setup window right away. Nothing is
/// saved here; the choices are written only when setup finishes with Done.
unsafe fn live(h: HWND, theme: Option<usize>, accent: Option<usize>) {
    if let Some(t) = theme {
        crate::ui::set_theme(t);
    }
    if let Some(a) = accent {
        crate::ui::set_accent(a);
    }
    InvalidateRect(h, std::ptr::null(), 0);
    EnumChildWindows(h, Some(crate::ui::invalidate_child), 0);
}
unsafe fn edit_text(h: HWND) -> String {
    let c = GetDlgItem(h, 101);
    let mut b = [0u16; 2048];
    let n = GetWindowTextW(c, b.as_mut_ptr(), 2048).max(0) as usize;
    String::from_utf16_lossy(&b[..n])
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_COMMAND => {
            let id = w & 65535;
            let code = w >> 16;
            let step = GetPropW(h, wide("guide-step").as_ptr()) as usize;
            if id == MIC_ID {
                MIC.fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                InvalidateRect(l as HWND, std::ptr::null(), 0);
            }
            if (CHIP_FIRST..CHIP_FIRST + crate::welcome_logic::COMMON_APPS.len()).contains(&id) {
                let names = crate::welcome_logic::COMMON_APPS[id - CHIP_FIRST].1;
                let next = crate::welcome_logic::toggle_group(&edit_text(h), names);
                SetWindowTextW(GetDlgItem(h, 101), wide(&next).as_ptr());
            }
            if id == 101 && code == EN_CHANGE as usize && step == 4 {
                for i in 0..crate::welcome_logic::COMMON_APPS.len() {
                    InvalidateRect(GetDlgItem(h, (CHIP_FIRST + i) as i32), std::ptr::null(), 0);
                }
            }
            if id == FOLDER_ID {
                let cur = FOLDER.lock().unwrap().clone().unwrap_or_default();
                match crate::save_folder::choose(h, &cur) {
                    Ok(Some(p)) => {
                        *FOLDER.lock().unwrap() = Some(p);
                        FOLDER_CHANGED.store(true, Relaxed);
                        InvalidateRect(l as HWND, std::ptr::null(), 0);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        SetWindowTextW(GetDlgItem(h, HINT_ID as i32), wide(&format!("Folder not changed: {e}")).as_ptr());
                    }
                }
            }
            if (SHARE_FIRST..SHARE_FIRST + draw::SHARE_LABELS.len()).contains(&id) {
                SHARE.store(id - SHARE_FIRST, Relaxed);
                for i in 0..draw::SHARE_LABELS.len() {
                    InvalidateRect(GetDlgItem(h, (SHARE_FIRST + i) as i32), std::ptr::null(), 0);
                }
            }
            if (1001..=1003).contains(&id) {
                SetPropW(h, wide("action").as_ptr(), (id - 1000) as _);
            }
            0
        }
        WM_CLOSE => {
            SetPropW(h, wide("action").as_ptr(), 2usize as _);
            0
        }
        WM_LBUTTONUP => {
            let step = GetPropW(h, wide("guide-step").as_ptr()) as usize;
            let x = l as i16 as i32;
            let y = (l >> 16) as i16 as i32;
            let (count, gap) = if step == 1 { (draw::LAYOUT_COUNT, draw::LAYOUT_GAP) } else if step == 2 { (6, 10) } else { (0, 0) };
            let (top, height) = if step == 1 { (draw::LAYOUT_TOP, draw::LAYOUT_H) } else { (draw::CARD_TOP, draw::CARD_H) };
            if count > 0 && (top..top + height).contains(&y) {
                if let Some(i) = crate::welcome_logic::card_at(x, 28, 688, count, gap) {
                    pick(h, step, i);
                }
            }
            if step == 5 {
                if let Some(i) = draw::preset_at(x, y) {
                    PRESET.store(i, Relaxed);
                    InvalidateRect(h, std::ptr::null(), 0);
                }
            }
            if step == 8 {
                let u = draw::upload_rect();
                if x >= u.0 && x < u.2 && y >= u.1 && y < u.3 {
                    let hint = GetDlgItem(h, HINT_ID as i32);
                    match crate::icon_upload::pick(h) {
                        Ok(Some(px)) => {
                            *upload().lock().unwrap() = Some(px);
                            ICON.store(crate::identity::custom_choice(), Relaxed);
                            SetWindowTextW(hint, wide("Image ready. It is saved when you press Done.").as_ptr());
                        }
                        Ok(None) => {}
                        Err(e) => {
                            SetWindowTextW(hint, wide(&format!("Image not used: {e}")).as_ptr());
                        }
                    }
                    InvalidateRect(h, std::ptr::null(), 0);
                } else if let Some(i) = draw::icon_at(x, y) {
                    ICON.store(i, Relaxed);
                    InvalidateRect(h, std::ptr::null(), 0);
                }
            }
            if step == 2 {
                if draw::in_rect(draw::custom_rect(), x, y) {
                    custom_editor(h);
                    return 0;
                }
                if let Some(i) = draw::accent_at(x, y) {
                    ACCENT.store(i, Relaxed);
                    live(h, None, Some(i));
                }
            }
            0
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
            let mut all = RECT::default();
            GetClientRect(h, &mut all);
            let step = GetPropW(h, wide("guide-step").as_ptr()) as usize;
            let v = view(h, step.max(1) - 1);
            exec(dc, &draw::base(&v), 0, 0);
            crate::ui::end_buffer(h, &ps, dc, b);
            0
        }
        WM_DRAWITEM => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            if d.CtlType == ODT_COMBOBOX {
                let t = crate::ui::style_theme();
                let selected = d.itemState & ODS_SELECTED != 0;
                FillRect(
                    d.hDC,
                    &d.rcItem,
                    crate::ui::style::brush(if selected { t.hover } else { t.panel }),
                );
                let mut label = [0u16; 256];
                let n = if d.itemID != u32::MAX {
                    SendMessageW(
                        d.hwndItem,
                        CB_GETLBTEXT,
                        d.itemID as usize,
                        label.as_mut_ptr() as isize,
                    )
                    .max(0)
                    .min(255) as i32
                } else {
                    0
                };
                let old = SelectObject(
                    d.hDC,
                    crate::ui::style::font(&crate::display::font_name(), 14, 400),
                );
                SetTextColor(d.hDC, t.text);
                SetBkMode(d.hDC, TRANSPARENT as _);
                let mut r = d.rcItem;
                r.left += 10;
                DrawTextW(d.hDC, label.as_ptr(), n, &mut r, DT_SINGLELINE | DT_VCENTER);
                SelectObject(d.hDC, old);
                return 1;
            }
            let mut text = [0u16; 80];
            let n = GetWindowTextW(d.hwndItem, text.as_mut_ptr(), 80);
            let title = String::from_utf16_lossy(&text[..n.max(0) as usize]);
            let t = crate::ui::style_theme();
            if d.CtlID as usize == MIC_ID {
                // On/off switch row.
                FillRect(d.hDC, &d.rcItem, crate::ui::style::brush(t.bg));
                let on = MIC.load(std::sync::atomic::Ordering::Relaxed);
                let r = d.rcItem;
                crate::ui::style::rect(
                    d.hDC,
                    r,
                    t.panel,
                    if GetFocus() == d.hwndItem { t.accent } else { t.border },
                    12,
                );
                let font = crate::display::font_name();
                crate::ui::style::label(
                    d.hDC,
                    &title,
                    RECT {
                        left: r.left + 18,
                        top: r.top,
                        right: r.right - 130,
                        bottom: r.bottom,
                    },
                    crate::ui::style::font(&font, 15, 600),
                    t.text,
                    DT_SINGLELINE | DT_VCENTER,
                );
                let mid = (r.top + r.bottom) / 2;
                let pill = RECT {
                    left: r.right - 18 - 48,
                    top: mid - 13,
                    right: r.right - 18,
                    bottom: mid + 13,
                };
                let fill = if on { t.accent } else { t.border };
                crate::ui::style::rect(d.hDC, pill, fill, fill, 13);
                let knob = if on {
                    RECT {
                        left: pill.right - 3 - 20,
                        top: mid - 10,
                        right: pill.right - 3,
                        bottom: mid + 10,
                    }
                } else {
                    RECT {
                        left: pill.left + 3,
                        top: mid - 10,
                        right: pill.left + 3 + 20,
                        bottom: mid + 10,
                    }
                };
                let kc = if on { t.bg } else { t.text };
                crate::ui::style::rect(d.hDC, knob, kc, kc, 10);
                crate::ui::style::label(
                    d.hDC,
                    if on { "On" } else { "Off" },
                    RECT {
                        left: pill.left - 60,
                        top: r.top,
                        right: pill.left - 10,
                        bottom: r.bottom,
                    },
                    crate::ui::style::font(&font, 14, 400),
                    if on { t.accent } else { t.muted },
                    DT_SINGLELINE | DT_VCENTER | DT_RIGHT,
                );
                return 1;
            }
            if d.CtlID as usize == FOLDER_ID {
                FillRect(d.hDC, &d.rcItem, crate::ui::style::brush(t.bg));
                let r = d.rcItem;
                crate::ui::style::rect(d.hDC, r, t.panel, if GetFocus() == d.hwndItem { t.accent } else { t.border }, 12);
                let font = crate::display::font_name();
                let path = FOLDER.lock().unwrap().clone().unwrap_or_default().to_string_lossy().to_string();
                crate::ui::style::label(
                    d.hDC,
                    &path,
                    RECT { left: r.left + 18, top: r.top, right: r.right - 110, bottom: r.bottom },
                    crate::ui::style::font(&font, 14, 400),
                    t.text,
                    DT_SINGLELINE | DT_VCENTER | DT_PATH_ELLIPSIS,
                );
                crate::ui::style::label(
                    d.hDC,
                    "Change",
                    RECT { left: r.right - 100, top: r.top, right: r.right - 18, bottom: r.bottom },
                    crate::ui::style::font(&font, 14, 600),
                    t.accent,
                    DT_SINGLELINE | DT_VCENTER | DT_RIGHT,
                );
                return 1;
            }
            if (SHARE_FIRST..SHARE_FIRST + draw::SHARE_LABELS.len()).contains(&(d.CtlID as usize)) {
                FillRect(d.hDC, &d.rcItem, crate::ui::style::brush(t.bg));
                let i = d.CtlID as usize - SHARE_FIRST;
                crate::ui::style::button(
                    d.hDC,
                    d.rcItem,
                    draw::SHARE_LABELS[i],
                    if SHARE.load(Relaxed) == i { crate::ui::style::Button::Primary } else { crate::ui::style::Button::Default },
                    crate::ui::style::State {
                        pressed: d.itemState & ODS_SELECTED != 0,
                        focused: GetFocus() == d.hwndItem,
                        ..Default::default()
                    },
                    t,
                    crate::ui::style::font(&crate::display::font_name(), 14, 400),
                );
                return 1;
            }
            if (CHIP_FIRST..CHIP_FIRST + crate::welcome_logic::COMMON_APPS.len())
                .contains(&(d.CtlID as usize))
            {
                // Common voice-chat app chip: shared draw list (avatar, name, check).
                let i = d.CtlID as usize - CHIP_FIRST;
                let names = crate::welcome_logic::COMMON_APPS[i].1;
                let chosen = crate::welcome_logic::group_selected(&edit_text(h), names);
                let v = view(h, 3);
                let w = d.rcItem.right - d.rcItem.left;
                let hh = d.rcItem.bottom - d.rcItem.top;
                FillRect(d.hDC, &d.rcItem, crate::ui::style::brush(v.p()[0]));
                exec(d.hDC, &draw::chip(&v, i, chosen, w, hh), d.rcItem.left, d.rcItem.top);
                return 1;
            }
            if (1001..=1003).contains(&d.CtlID) {
                FillRect(d.hDC, &d.rcItem, crate::ui::style::brush(t.bg));
                crate::ui::style::button(
                    d.hDC,
                    d.rcItem,
                    &title,
                    if d.CtlID == 1001 {
                        crate::ui::style::Button::Primary
                    } else {
                        crate::ui::style::Button::Default
                    },
                    crate::ui::style::State {
                        pressed: d.itemState & ODS_SELECTED != 0,
                        focused: GetFocus() == d.hwndItem,
                        disabled: d.itemState & ODS_DISABLED != 0,
                        ..Default::default()
                    },
                    t,
                    crate::ui::style::font(&crate::display::font_name(), 14, 400),
                );
                1
            } else {
                crate::ui::handle(h, m, w, l).unwrap_or(0)
            }
        }
        WM_CTLCOLORSTATIC => {
            SetTextColor(w as _, crate::ui::text_color());
            SetBkColor(w as _, crate::ui::bg());
            crate::ui::style::brush(crate::ui::bg()) as isize
        }
        _ => crate::ui::handle(h, m, w, l).unwrap_or_else(|| DefWindowProcW(h, m, w, l)),
    }
}
unsafe fn child(h: HWND, class: &str, t: &str, id: usize, y: i32, font: HFONT) -> HWND {
    let style = WS_CHILD
        | WS_VISIBLE
        | if class == "BUTTON" {
            WS_TABSTOP | BS_OWNERDRAW as u32
        } else if class == "COMBOBOX" {
            WS_TABSTOP
                | CBS_DROPDOWNLIST as u32
                | WS_VSCROLL
                | CBS_OWNERDRAWFIXED as u32
                | CBS_HASSTRINGS as u32
        } else if class == "EDIT" {
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32
        } else {
            0
        };
    let c = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(t).as_ptr(),
        style,
        28,
        y,
        688,
        if class == "COMBOBOX" { 280 } else { 40 },
        h,
        id as _,
        std::ptr::null_mut(),
        std::ptr::null(),
    );
    SendMessageW(c, WM_SETFONT, font as usize, 0);
    if class == "BUTTON" {
        crate::ui::interactive(c);
    }
    c
}
pub unsafe fn show(root: &Path) {
    if root.join("welcome-complete.txt").exists() {
        return;
    }
    let inst = GetModuleHandleW(std::ptr::null());
    let cls = wide("ZimClipWelcome");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let h = CreateWindowExW(
        0,
        cls.as_ptr(),
        wide("Welcome to ZimClip").as_ptr(),
        WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
        160,
        {
            // Keep the whole window on the work area: start at the top when the screen is short.
            let mut work: RECT = std::mem::zeroed();
            SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as *mut _, 0);
            if work.bottom > work.top {
                (work.bottom - 640).min(40).max(work.top)
            } else {
                40
            }
        },
        760,
        640,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    let font = CreateFontW(
        -14,
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
        wide(&crate::display::font_name()).as_ptr(),
    );
    crate::ui::titlebar(h);
    let description = child(h, "STATIC", "", 0, 142, font);
    crate::display::put(description, 28, 134, 688, 48);
    let hint = child(h, "STATIC", "", HINT_ID, 484, font);
    crate::display::put(hint, 28, 482, 688, 34);
    let skip = child(h, "BUTTON", "Skip setup", 1002, 20, font);
    crate::display::put(skip, 588, 28, 128, 34);
    let back = child(h, "BUTTON", "Back", 1003, 534, font);
    // On a short work area the bottom row would sit off screen, so Back and Next go to the header
    // area under the title, right side. Normal screens keep the bottom row.
    let short = {
        let mut work: RECT = std::mem::zeroed();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as *mut _, 0);
        work.bottom > work.top && work.bottom - work.top < 650
    };
    // Short screens: below the title band (ends y 72) and above the header divider (y 122), right side,
    // clear of the progress dots on the left.
    crate::display::put(back, 468, if short { 80 } else { 534 }, if short { 110 } else { 108 }, if short { 34 } else { 40 });
    let next = child(h, "BUTTON", "Next", 1001, 534, font);
    crate::display::put(next, if short { 586 } else { 588 }, if short { 80 } else { 534 }, if short { 130 } else { 128 }, if short { 34 } else { 40 });
    let mut capture = crate::capture_settings::CaptureSettings::load(root);
    let prefs = std::fs::read_to_string(root.join("personalization.ini")).unwrap_or_default();
    let value = |k: &str, d: usize| {
        prefs
            .lines()
            .find_map(|l| {
                l.split_once('=')
                    .filter(|(n, _)| *n == k)
                    .and_then(|(_, v)| v.parse().ok())
            })
            .unwrap_or(d)
    };
    let mut layout = std::fs::read_to_string(root.join("layout.txt"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0usize)
        .min(4);
    let mut theme = value("theme", 0) % 7;
    crate::custom_theme::load(root);
    let first_custom = crate::custom_theme::current();
    *PRESETS.lock().unwrap() = crate::custom_theme::load_presets(root);
    CUSTOM_DIRTY.store(false, Relaxed);
    let mut accent = std::fs::read_to_string(root.join("appearance.ini"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0usize)
        % 8;
    let mut apps = std::fs::read_to_string(root.join("voice-chat-apps.txt")).unwrap_or_default();
    let mut key = value("hotkey", 0x447) as u32;
    let first_theme = crate::ui::theme_index();
    let first_accent = crate::ui::accent_index();
    let mut pos = 0usize;
    LAYOUT.store(layout, Relaxed);
    THEME.store(theme, Relaxed);
    ACCENT.store(accent, Relaxed);
    PRESET.store(draw::preset_for(capture.width, capture.fps).unwrap_or(usize::MAX), Relaxed);
    ICON.store(crate::identity::icon_choice(), Relaxed);
    *FOLDER.lock().unwrap() = Some(crate::save_folder::load(root));
    FOLDER_CHANGED.store(false, Relaxed);
    SHARE.store(
        crate::file_prefs::share_default_mb(root)
            .and_then(|mb| crate::file_prefs::SHARE_SIZES_MB.iter().position(|v| *v == mb))
            .map(|i| i + 1)
            .unwrap_or(0),
        Relaxed,
    );
    let clip_secs = std::fs::read_to_string(root.join("buffer-seconds.txt"))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|v| (5..=300).contains(v))
        .unwrap_or(30);
    *SECS.lock().unwrap() = None;
    *DEV_PICK.lock().unwrap() = [None, None];
    let mut name = crate::identity::name();
    let mut shown = usize::MAX;
    let mut controls = Vec::<HWND>::new();
    ShowWindow(h, SW_SHOW);
    loop {
        let step = draw::ORDER[pos];
        POS.store(pos, Relaxed);
        if shown != pos {
            // Capture the page being left so it can fade out over the new one.
            let fade = if shown == usize::MAX { None } else { crate::ui::transition::snapshot(h) };
            for c in controls.drain(..) {
                DestroyWindow(c);
            }
            shown = pos;
            SetPropW(h, wide("guide-step").as_ptr(), (step + 1) as _);
            InvalidateRect(h, std::ptr::null(), 0);
            SetWindowTextW(
                hint,
                wide("Choices are saved on Done. Change them later in Settings.").as_ptr(),
            );
            EnableWindow(back, if pos > 0 { 1 } else { 0 });
            ShowWindow(back, if pos > 0 { SW_SHOW } else { SW_HIDE });
            SetWindowTextW(next, wide(if pos == draw::PAGES - 1 { "Done" } else if pos == 0 { "Let's go" } else { "Next" }).as_ptr());
            let text=match step{6=>"",7=>"",0=>"",1=>"Click a theme or pick an accent to preview it here. It is saved when you press Done.",2=>"Each source saves to its own track. Mic access is checked when capture starts.",8=>"",3=>"Chosen apps record to the Comms track. None means no isolation.\nA saved list, not live detection.",4=>"Click the box, then press a key combination. Next checks Windows can register it.\nAvoid keys your game already uses.",_=>""};
            SetWindowTextW(description, wide(text).as_ptr());
            // A static control paints its whole rectangle, so an empty one must be hidden:
            // it would blank whatever the page draws under it (logo, labels, cards).
            ShowWindow(description, if text.is_empty() { SW_HIDE } else { SW_SHOW });
            ShowWindow(hint, if step == 6 { SW_HIDE } else { SW_SHOW });
            if step == 4 {
                { let t = draw::HOTKEY_TEXT; crate::display::put(description, t.0, t.1, t.2 - t.0, t.3 - t.1); }
            } else {
                crate::display::put(description, 28, 134, 688, 48);
            }
            match step {
                // Layout, theme and welcome pages: cards and dots are painted and clickable.
                0 | 1 | 6 => {}
                7 => {
                    let c = child(h, "EDIT", &name, NAME_ID, 140, font);
                    SetPropW(c, wide("no-cue").as_ptr(), 1usize as _);
                    SendMessageW(c, EM_SETLIMITTEXT, 24, 0);
                    crate::ui::search_field(c);
                    { let f = draw::NAME_FIELD; crate::display::put(c, f.0, f.1, f.2 - f.0, f.3 - f.1); }
                    controls.push(c);
                }
                2 => {
                    MIC.store(capture.microphone, std::sync::atomic::Ordering::Relaxed);
                    let sw = child(h, "BUTTON", "Record my microphone", MIC_ID, 190, font);
                    crate::display::put(sw, 28, 190, 688, 56);
                    crate::ui::tooltip(sw, "Turn microphone recording on or off");
                    controls.push(sw);
                    let saved = crate::audio_devices::selected();
                    let mut lists = Vec::new();
                    for (k, mic) in [true, false].into_iter().enumerate() {
                        let mut list = crate::audio_devices::list(mic).unwrap_or_else(|e| {
                            vec![(String::new(), format!("Windows default (list unavailable: {e})"))]
                        });
                        let id = DEV_PICK.lock().unwrap()[k].clone().unwrap_or_else(|| if mic { saved.mic.clone() } else { saved.system.clone() });
                        if !id.is_empty() && !list.iter().any(|d| d.0 == id) {
                            list.push((id.clone(), "Saved device unavailable (not replaced)".into()));
                        }
                        let y = if mic { 282 } else { 370 };
                        let c = child(h, "COMBOBOX", "", DEV_FIRST + k, y, font);
                        crate::ui::dropdown(c);
                        for (_, n) in &list {
                            SendMessageW(c, CB_ADDSTRING, 0, wide(n).as_ptr() as isize);
                        }
                        let index = list.iter().position(|d| d.0 == id).unwrap_or(0);
                        SendMessageW(c, CB_SETCURSEL, index, 0);
                        controls.push(c);
                        lists.push(list);
                    }
                    *DEVICES.lock().unwrap() = lists;
                }
                8 => {
                    let b = child(h, "BUTTON", "", FOLDER_ID, 152, font);
                    { let r = draw::FOLDER_BTN; crate::display::put(b, r.0, r.1, r.2 - r.0, r.3 - r.1); }
                    crate::ui::tooltip(b, "Choose the folder clips are saved in");
                    controls.push(b);
                    let shown = SECS.lock().unwrap().unwrap_or(clip_secs).to_string();
                    let e = child(h, "EDIT", &shown, SECS_ID, 250, font);
                    SetPropW(e, wide("no-cue").as_ptr(), 1usize as _);
                    SendMessageW(e, EM_SETLIMITTEXT, 3, 0);
                    crate::ui::search_field(e);
                    { let r = draw::SECS_EDIT; crate::display::put(e, r.0, r.1, r.2 - r.0, r.3 - r.1); }
                    controls.push(e);
                    for i in 0..draw::SHARE_LABELS.len() {
                        let c = child(h, "BUTTON", draw::SHARE_LABELS[i], SHARE_FIRST + i, draw::SHARE_CHIP_Y, font);
                        { let r = draw::share_chip(i); crate::display::put(c, r.0, r.1, r.2 - r.0, r.3 - r.1); }
                        controls.push(c);
                    }
                    SetWindowTextW(hint, wide("All of these can be changed later: folder and share size in Settings > Files / app, replay length on the Home screen.").as_ptr());
                }
                3 => {
                    // The text box stays first: Next reads the saved list from it.
                    let edit = child(h, "EDIT", &apps, 101, 358, font);
                    SetPropW(edit, wide("no-cue").as_ptr(), 1usize as _);
                    crate::ui::search_field(edit);
                    crate::display::put(edit, 28, 388, 688, 44);
                    controls.push(edit);
                    let top = child(h, "STATIC", "Most common apps. Click to add or remove.", 0, 206, font);
                    crate::display::put(top, 28, 192, 688, 24);
                    controls.push(top);
                    for (i, (name, _)) in crate::welcome_logic::COMMON_APPS.iter().enumerate() {
                        let chip = child(h, "BUTTON", name, CHIP_FIRST + i, 232, font);
                        let (col, row) = ((i % 4) as i32, (i / 4) as i32);
                        { let r = draw::chip_rect(i); crate::display::put(chip, r.0, r.1, r.2 - r.0, r.3 - r.1); let _ = (col, row); }
                        controls.push(chip);
                    }
                    let other = child(h, "STATIC", "Other apps. Type file names separated by commas.", 0, 332, font);
                    crate::display::put(other, 28, 358, 688, 24);
                    controls.push(other);
                }
                4 => {
                    let c = child(h, "msctls_hotkey32", "", 101, 228, font);
                    SendMessageW(c, 0x401, key as usize, 0);
                    // Same dark rounded field as the Settings shortcut box.
                    crate::ui::modern_hotkey(c);
                    { let f = draw::HOTKEY_FIELD; crate::display::put(c, f.0, f.1, f.2 - f.0, f.3 - f.1); }
                    controls.push(c);
                }
                _ => {
                    let accents = [
                        "Mint", "Sky", "Lilac", "Amber", "Ice", "Peach", "Rose", "Silver",
                    ];
                    let themes = ["Charcoal", "Midnight", "Plum", "Ember", "Forest", "High contrast", "Custom"];
                    let (cw, cf) = match PRESET.load(Relaxed) {
                        i if i < 3 => (draw::PRESETS[i].1, draw::PRESETS[i].2),
                        _ => (capture.width, capture.fps),
                    };
                    let capture_text = match PRESET.load(Relaxed) {
                        i if i < 3 => format!("{}, {cw} wide, {cf} fps", draw::PRESETS[i].0),
                        _ => format!("{cw} wide, {cf} fps"),
                    };
                    let apps_text = if apps.is_empty() {
                        "None".to_string()
                    } else if apps.chars().count() > 26 {
                        format!("{}...", apps.chars().take(23).collect::<String>())
                    } else {
                        apps.clone()
                    };
                    *SUMMARY.lock().unwrap() = vec![
                        draw::LAYOUT_NAMES[layout.min(4)].to_string(),
                        format!("{} / {}", themes[theme % 7], accents[accent % 8]),
                        format!("System {} / Mic {}", if capture.system_audio { "on" } else { "off" }, if capture.microphone { "on" } else { "off" }),
                        apps_text,
                        crate::shortcuts::label(key),
                        capture_text,
                        {
                            let secs = SECS.lock().unwrap().unwrap_or(clip_secs);
                            match SHARE.load(Relaxed) {
                                0 => format!("{secs} s replay"),
                                i => format!("{secs} s, share {} MB", crate::file_prefs::SHARE_SIZES_MB[i - 1]),
                            }
                        },
                        name.clone(),
                    ];
                    InvalidateRect(h, std::ptr::null(), 0);
                }
            }
            if let Some(f) = fade {
                UpdateWindow(h);
                crate::ui::transition::play(h, f);
            }
        }
        let action = GetPropW(h, wide("action").as_ptr()) as usize;
        RemovePropW(h, wide("action").as_ptr());
        if action == 2 {
            // Previews were never saved: put the colors back as they were.
            crate::ui::set_theme(first_theme);
            crate::ui::set_accent(first_accent);
            crate::custom_theme::set_current(first_custom);
            match std::fs::write(root.join("welcome-complete.txt"), "skipped") {
                Ok(()) => break,
                Err(e) => {
                    SetWindowTextW(
                        hint,
                        wide(&format!(
                            "Setup skipped, but could not save this choice: {e}"
                        ))
                        .as_ptr(),
                    );
                    break;
                }
            }
        }
        if action == 1 || action == 3 {
            let mut valid = true;
            match step {
                7 => {
                    let mut b = [0u16; 64];
                    let n = GetWindowTextW(controls[0], b.as_mut_ptr(), 64).max(0) as usize;
                    match crate::identity::validate(&String::from_utf16_lossy(&b[..n])) {
                        Ok(n) => name = n,
                        Err(e) => {
                            SetWindowTextW(hint, wide(e).as_ptr());
                            valid = false;
                        }
                    }
                }
                0 => layout = LAYOUT.load(Relaxed),
                1 => {
                    theme = THEME.load(Relaxed);
                    accent = ACCENT.load(Relaxed);
                }
                2 => {
                    capture.microphone = MIC.load(std::sync::atomic::Ordering::Relaxed);
                    let lists = DEVICES.lock().unwrap().clone();
                    for k in 0..2 {
                        let sel = SendMessageW(controls[1 + k], CB_GETCURSEL, 0, 0);
                        if sel >= 0 {
                            if let Some(d) = lists.get(k).and_then(|l| l.get(sel as usize)) {
                                DEV_PICK.lock().unwrap()[k] = Some(d.0.clone());
                            }
                        }
                    }
                }
                8 => {
                    let mut b = [0u16; 8];
                    let n = GetWindowTextW(controls[1], b.as_mut_ptr(), 8).max(0) as usize;
                    match String::from_utf16_lossy(&b[..n]).trim().parse::<u64>() {
                        Ok(v) if (5..=300).contains(&v) => *SECS.lock().unwrap() = Some(v),
                        _ => {
                            SetWindowTextW(hint, wide("Clip length must be 5 to 300 seconds.").as_ptr());
                            valid = false;
                        }
                    }
                }
                3 => {
                    let mut b = [0u16; 2048];
                    let n = GetWindowTextW(controls[0], b.as_mut_ptr(), 2048);
                    let t = String::from_utf16_lossy(&b[..n as usize]);
                    match crate::comms::normalize(&t) {
                        Ok(n) => apps = n.join(", "),
                        Err(e) => {
                            SetWindowTextW(hint, wide(&e).as_ptr());
                            valid = false;
                        }
                    }
                }
                4 => {
                    let k = SendMessageW(controls[0], 0x402, 0, 0) as u32;
                    let vk = k & 255;
                    let flags = (k >> 8) & 7;
                    let modifiers = (if flags & 1 != 0 { MOD_SHIFT } else { 0 })
                        | (if flags & 2 != 0 { MOD_CONTROL } else { 0 })
                        | (if flags & 4 != 0 { MOD_ALT } else { 0 });
                    let pause_now = crate::file_prefs::pause_key(
                        crate::file_prefs::prefs_file().parent().unwrap_or(std::path::Path::new(".")),
                    );
                    if let Err(e) = crate::shortcuts::validate(crate::shortcuts::normalize(k), pause_now, "Pause") {
                        valid = false;
                        SetWindowTextW(hint, wide(&e).as_ptr());
                    } else {
                        if action == 1
                            && RegisterHotKey(
                                std::ptr::null_mut(),
                                98,
                                modifiers | MOD_NOREPEAT,
                                vk,
                            ) == 0
                        {
                            valid = false;
                            SetWindowTextW(
                                hint,
                                wide("That key is in use. Choose another, or Skip setup.").as_ptr(),
                            );
                        } else {
                            UnregisterHotKey(std::ptr::null_mut(), 98);
                            key = k;
                        }
                    }
                }
                _ => {}
            }
            if valid || action == 3 {
                if action == 3 {
                    pos = pos.saturating_sub(1);
                } else if pos < draw::PAGES - 1 {
                    pos += 1;
                } else {
                    let r = (|| -> Result<(), String> {
                        if let Some(i) = (PRESET.load(Relaxed) < 3).then(|| PRESET.load(Relaxed)) {
                            capture.width = draw::PRESETS[i].1;
                            capture.fps = draw::PRESETS[i].2;
                        }
                        capture.save(root)?;
                        crate::identity::save(root, &name)?;
                        let icon = ICON.load(Relaxed);
                        let image = upload().lock().unwrap().clone();
                        crate::identity::save_icon(root, icon, image.as_deref())?;
                        crate::comms::save(root, &apps)?;
                        crate::welcome_logic::write_audio_and_clips(
                            root,
                            DEV_PICK.lock().unwrap().clone(),
                            FOLDER_CHANGED.load(Relaxed).then(|| FOLDER.lock().unwrap().clone()).flatten().as_deref(),
                            *SECS.lock().unwrap(),
                            clip_secs,
                            SHARE.load(Relaxed),
                        )?;
                        if CUSTOM_DIRTY.load(Relaxed) || theme == crate::custom_theme::THEME_INDEX {
                            crate::custom_theme::save_custom(root, crate::custom_theme::current())?;
                            crate::custom_theme::save_presets(root, &PRESETS.lock().unwrap())?;
                        }
                        std::fs::write(root.join("layout.txt"), layout.to_string())
                            .map_err(|e| e.to_string())?;
                        std::fs::write(root.join("appearance.ini"), accent.to_string())
                            .map_err(|e| e.to_string())?;
                        std::fs::write(
                            root.join("personalization.ini"),
                            format!(
                                "theme={theme}\nhotkey={key}\nsound={}\ntoast={}\nui={}\nmotion={}\n",
                                value("sound", 1),
                                value("toast", 1),
                                if crate::polished::enabled() { "polished" } else { "legacy" },
                                crate::polished::motion_value()
                            ),
                        )
                        .map_err(|e| e.to_string())?;
                        std::fs::write(root.join("welcome-complete.txt"), "done")
                            .map_err(|e| e.to_string())
                    })();
                    match r {
                        Ok(()) => break,
                        Err(e) => {
                            SetWindowTextW(
                                hint,
                                wide(&format!("Could not save setup: {e}")).as_ptr(),
                            );
                        }
                    }
                }
            }
        }
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    DestroyWindow(h);
    DeleteObject(font);
}
