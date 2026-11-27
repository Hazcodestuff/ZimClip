use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[link(name = "winmm")]
unsafe extern "system" {
    fn PlaySoundW(data: *const u16, module: *mut core::ffi::c_void, flags: u32) -> i32;
}
pub unsafe fn ding() {
    #[repr(align(4))]
    struct Aligned([u8; include_bytes!("../assets/clip-ding.wav").len()]);
    static BYTES: Aligned = Aligned(*include_bytes!("../assets/clip-ding.wav"));
    let bytes = &BYTES.0;
    PlaySoundW(
        bytes.as_ptr() as _,
        std::ptr::null_mut(),
        0x0004 | 0x0001 | 0x0002,
    );
}
struct Toast {
    start: std::time::Instant,
    exiting: bool,
    x: i32,
    y: i32,
    motion: bool,
}
unsafe fn state(h: HWND) -> *mut Toast {
    GetPropW(h, wide("toast-state").as_ptr()) as *mut Toast
}
unsafe fn frame(h: HWND) {
    let p = state(h);
    if p.is_null() {
        return;
    }
    let s = &*p;
    let mut enabled = 0i32;
    if SystemParametersInfoW(0x1042, 0, &mut enabled as *mut _ as _, 0) == 0 || enabled == 0 {
        KillTimer(h, 2);
        if s.exiting {
            DestroyWindow(h);
        } else {
            SetLayeredWindowAttributes(h, 0, 255, LWA_ALPHA);
            SetWindowPos(
                h,
                std::ptr::null_mut(),
                s.x,
                s.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        return;
    }
    let t = (s.start.elapsed().as_secs_f32() / 0.18).clamp(0., 1.);
    let t = 1. - (1. - t).powi(3);
    let v = if s.exiting { 1. - t } else { t };
    SetLayeredWindowAttributes(h, 0, (255. * v).round() as u8, LWA_ALPHA);
    SetWindowPos(
        h,
        std::ptr::null_mut(),
        s.x,
        s.y + ((1. - v) * 12.).round() as i32,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    if t >= 1. {
        KillTimer(h, 2);
        if s.exiting {
            DestroyWindow(h);
        }
    }
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as _,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
            let t = crate::ui::style_theme();
            crate::ui::style::rect(
                dc,
                RECT {
                    left: 0,
                    top: 0,
                    right: 342,
                    bottom: 88,
                },
                t.panel,
                t.border,
                16,
            );
            crate::ui::style::rect(
                dc,
                RECT {
                    left: 16,
                    top: 24,
                    right: 56,
                    bottom: 64,
                },
                crate::ui::style::mix(t.panel, t.accent, 16),
                crate::ui::style::mix(t.panel, t.accent, 16),
                20,
            );
            let p = SelectObject(dc, crate::ui::style::pen(t.accent, 2) as _);
            MoveToEx(dc, 27, 44, std::ptr::null_mut());
            LineTo(dc, 34, 51);
            LineTo(dc, 46, 37);
            SelectObject(dc, p);
            let face = crate::display::font_name();
            crate::ui::style::label(
                dc,
                "Replay saved",
                RECT {
                    left: 70,
                    top: 19,
                    right: 326,
                    bottom: 44,
                },
                crate::ui::style::font(&face, 18, 600),
                t.text,
                DT_SINGLELINE | DT_VCENTER,
            );
            crate::ui::style::label(
                dc,
                "Ready in your clip library",
                RECT {
                    left: 70,
                    top: 46,
                    right: 326,
                    bottom: 68,
                },
                crate::ui::style::font(&face, 13, 400),
                t.muted,
                DT_SINGLELINE | DT_VCENTER,
            );
            crate::ui::end_buffer(h, &ps, dc, b);
            0
        }
        WM_TIMER if w == 1 => {
            KillTimer(h, 1);
            let p = state(h);
            if !p.is_null() && (*p).motion {
                (*p).exiting = true;
                (*p).start = std::time::Instant::now();
                if SetTimer(h, 2, 16, None) == 0 {
                    DestroyWindow(h);
                }
            } else {
                DestroyWindow(h);
            }
            0
        }
        WM_TIMER if w == 2 => {
            frame(h);
            0
        }
        WM_NCDESTROY => {
            KillTimer(h, 1);
            KillTimer(h, 2);
            let p = RemovePropW(h, wide("toast-state").as_ptr()) as *mut Toast;
            if !p.is_null() {
                drop(Box::from_raw(p));
            }
            DefWindowProcW(h, m, w, l)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
pub unsafe fn show(owner: HWND) {
    let class = wide("ZimClipSavedToast");
    let previous = FindWindowW(class.as_ptr(), std::ptr::null());
    if !previous.is_null() {
        DestroyWindow(previous);
    }
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        lpszClassName: class.as_ptr(),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let mut work = RECT::default();
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of_val(&info) as _;
    if GetMonitorInfoW(
        MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
        &mut info,
    ) != 0
    {
        work = info.rcWork;
    } else {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as _, 0);
    }
    let mut enabled = 0i32;
    let motion =
        SystemParametersInfoW(0x1042, 0, &mut enabled as *mut _ as _, 0) != 0 && enabled != 0;
    let x = work.right - 362;
    let y = work.top + 20;
    let h = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
        class.as_ptr(),
        wide("").as_ptr(),
        WS_POPUP,
        x,
        y,
        342,
        88,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null(),
    );
    if h.is_null() {
        return;
    }
    let p = Box::into_raw(Box::new(Toast {
        start: std::time::Instant::now(),
        exiting: false,
        x,
        y,
        motion,
    }));
    if SetPropW(h, wide("toast-state").as_ptr(), p as _) == 0 {
        drop(Box::from_raw(p));
        DestroyWindow(h);
        return;
    }
    crate::ui::clip_round(h, 32);
    SetWindowDisplayAffinity(h, 0x11);
    SetLayeredWindowAttributes(h, 0, if motion { 0 } else { 255 }, LWA_ALPHA);
    ShowWindow(h, SW_SHOWNOACTIVATE);
    if SetTimer(h, 1, 2300, None) == 0 {
        DestroyWindow(h);
        return;
    }
    if motion && SetTimer(h, 2, 16, None) == 0 {
        SetLayeredWindowAttributes(h, 0, 255, LWA_ALPHA);
    }
}
