//! Whole-page cross-fade. The old page is captured before the change; a click-through layered
//! window shows that picture above the new page and fades it out in ~150 ms. No timer or
//! window exists before or after the fade. Reduce motion, the Windows "animation effects"
//! setting, a disabled, hidden or minimised window all skip it (see `motion::allowed`).
use std::time::Instant;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::*,
};

const DURATION_MS: f32 = 150.0;
const TIMER: usize = 0x5a44;

pub struct Snap {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    w: i32,
    h: i32,
}
struct Fade {
    snap: Snap,
    since: Instant,
}
unsafe fn free(s: &Snap) {
    SelectObject(s.dc, s.old);
    DeleteObject(s.bmp as _);
    DeleteDC(s.dc);
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
/// Capture the window's client area, children included. None when motion is off or the
/// capture fails; callers just skip the fade then.
pub unsafe fn snapshot(h: HWND) -> Option<Snap> {
    if !super::motion::allowed(h) {
        return None;
    }
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let (w, hh) = (r.right, r.bottom);
    if w < 64 || hh < 64 || w > 4000 || hh > 3000 {
        return None;
    }
    let screen = GetDC(h);
    if screen.is_null() {
        return None;
    }
    let dc = CreateCompatibleDC(screen);
    let bmp = CreateCompatibleBitmap(screen, w, hh);
    ReleaseDC(h, screen);
    if dc.is_null() || bmp.is_null() {
        if !dc.is_null() {
            DeleteDC(dc);
        }
        if !bmp.is_null() {
            DeleteObject(bmp as _);
        }
        return None;
    }
    let old = SelectObject(dc, bmp as _);
    // Copy the pixels actually on screen (children included). PrintWindow cannot be relied on
    // for owner-drawn children, so only capture when this window is visibly on top.
    let mut origin = POINT { x: 0, y: 0 };
    ClientToScreen(h, &mut origin);
    // A fade from an earlier switch is still running (rapid switching). Capturing the screen
    // now would take the half-faded overlay as the "old page". End that fade and let this
    // page snap, so no stale picture stays above the new page.
    let cls = wide("ZimClipFade");
    let running = FindWindowW(cls.as_ptr(), std::ptr::null());
    if !running.is_null() {
        DestroyWindow(running);
        SelectObject(dc, old);
        DeleteObject(bmp as _);
        DeleteDC(dc);
        return None;
    }
    // The page must be on top at the centre and at all four corners.
    let ok = [(w / 2, hh / 2), (4, 4), (w - 5, 4), (4, hh - 5), (w - 5, hh - 5)].iter().all(|&(x, y)| {
        let top = WindowFromPoint(POINT { x: origin.x + x, y: origin.y + y });
        !top.is_null() && (top == h || IsChild(h, top) != 0)
    });
    let desk = GetDC(std::ptr::null_mut());
    let copied = ok && !desk.is_null() && BitBlt(dc, 0, 0, w, hh, desk, origin.x, origin.y, SRCCOPY) != 0;
    if !desk.is_null() {
        ReleaseDC(std::ptr::null_mut(), desk);
    }
    if !copied {
        SelectObject(dc, old);
        DeleteObject(bmp as _);
        DeleteDC(dc);
        return None;
    }
    Some(Snap { dc, bmp, old, w, h: hh })
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_NCHITTEST => HTTRANSPARENT as isize,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_TIMER if w == TIMER => {
            let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *const Fade;
            if p.is_null() {
                return 0;
            }
            let t = ((*p).since.elapsed().as_millis() as f32 / DURATION_MS).clamp(0., 1.);
            if t >= 1. || super::motion::reduced() {
                DestroyWindow(h);
            } else {
                let e = t * t * (3. - 2. * t);
                set_alpha(h, &(*p).snap, ((1. - e) * 255.) as u8);
            }
            0
        }
        WM_DESTROY => {
            KillTimer(h, TIMER);
            let p = SetWindowLongPtrW(h, GWLP_USERDATA, 0) as *mut Fade;
            if !p.is_null() {
                let f = Box::from_raw(p);
                free(&f.snap);
            }
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
unsafe fn set_alpha(h: HWND, s: &Snap, alpha: u8) {
    let size = SIZE { cx: s.w, cy: s.h };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: alpha, AlphaFormat: 0 };
    UpdateLayeredWindow(h, std::ptr::null_mut(), std::ptr::null(), &size, s.dc, &src, 0, &blend, ULW_ALPHA);
}
/// Show the captured old page over `parent` and fade it out. Takes ownership of the capture.
pub unsafe fn play(parent: HWND, snap: Snap) {
    let inst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let cls = wide("ZimClipFade");
    let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst, lpszClassName: cls.as_ptr(), ..std::mem::zeroed() };
    RegisterClassW(&wc);
    let mut origin = POINT { x: 0, y: 0 };
    ClientToScreen(parent, &mut origin);
    let ov = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        cls.as_ptr(),
        std::ptr::null(),
        WS_POPUP,
        origin.x,
        origin.y,
        snap.w,
        snap.h,
        parent,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    if ov.is_null() {
        free(&snap);
        return;
    }
    set_alpha(ov, &snap, 255);
    let f = Box::into_raw(Box::new(Fade { snap, since: Instant::now() }));
    SetWindowLongPtrW(ov, GWLP_USERDATA, f as isize);
    ShowWindow(ov, SW_SHOWNOACTIVATE);
    if SetTimer(ov, TIMER, 16, None) == 0 {
        DestroyWindow(ov);
    }
}
