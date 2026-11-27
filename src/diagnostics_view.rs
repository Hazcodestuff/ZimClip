use std::path::Path;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(h: HWND) -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, data: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn CloseClipboard() -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut core::ffi::c_void;
    fn GlobalLock(h: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn GlobalUnlock(h: *mut core::ffi::c_void) -> i32;
    fn GlobalFree(h: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
}
unsafe fn copy(h: HWND, s: &str) -> bool {
    let t = wide(s);
    let mem = GlobalAlloc(0x42, t.len() * 2);
    if mem.is_null() {
        return false;
    }
    let p = GlobalLock(mem);
    if p.is_null() {
        GlobalFree(mem);
        return false;
    }
    std::ptr::copy_nonoverlapping(t.as_ptr(), p as *mut u16, t.len());
    GlobalUnlock(mem);
    if OpenClipboard(h) == 0 {
        GlobalFree(mem);
        return false;
    }
    EmptyClipboard();
    let success = !SetClipboardData(13, mem).is_null();
    CloseClipboard();
    if !success {
        GlobalFree(mem);
    }
    success
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_CLOSE => {
            SetPropW(h, wide("action").as_ptr(), 3usize as _);
            0
        }
        WM_COMMAND => {
            if (401..=403).contains(&(w & 65535)) {
                SetPropW(h, wide("action").as_ptr(), (w & 65535) as _);
            }
            0
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            let br = CreateSolidBrush(crate::ui::bg());
            FillRect(dc, &r, br);
            DeleteObject(br);
            crate::ui::end_buffer(h, &ps, dc, b);
            0
        }
        WM_ERASEBKGND => 1,
        _ => crate::ui::handle(h, m, w, l).unwrap_or_else(|| DefWindowProcW(h, m, w, l)),
    }
}
pub unsafe fn show(parent: HWND, root: &Path, font: HFONT) {
    let path = root.join("diagnostics.txt");
    let log =
        std::fs::read_to_string(&path).unwrap_or_else(|e| format!("Cannot read diagnostics: {e}"));
    let editor = std::fs::read_to_string(root.join("editor-diagnostics.txt")).unwrap_or_default();
    let tail = |s: &str, n: usize| {
        s.lines()
            .rev()
            .take(n)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\r\n")
    };
    let report=format!("ZimClip {}\r\n\r\nCAPTURE LOG (latest 50 lines)\r\n{}\r\n\r\nWORKBENCH LOG (latest 20 lines)\r\n{}",env!("CARGO_PKG_VERSION"),tail(&log,50),tail(&editor,20));
    let inst = GetModuleHandleW(std::ptr::null());
    let cls = wide("ZimClipDiagnostics");
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    });
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
    let x = (r.left + ((r.right - r.left) - 850) / 2).clamp(
        info.rcWork.left,
        (info.rcWork.right - 850).max(info.rcWork.left),
    );
    let y = (r.top + ((r.bottom - r.top) - 500) / 2).clamp(
        info.rcWork.top,
        (info.rcWork.bottom - 500).max(info.rcWork.top),
    );
    let h = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        cls.as_ptr(),
        wide("Diagnostics").as_ptr(),
        WS_POPUP | WS_CLIPCHILDREN,
        x,
        y,
        850,
        500,
        parent,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    crate::ui::clip_round(h, 24);
    SetPropW(h, wide("surface-bg").as_ptr(), 1usize as _);
    let add = |class: &str, text: &str, id: usize, x: i32, y: i32, w: i32, z: i32, style: u32| {
        let c = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            x,
            y,
            w,
            z,
            h,
            id as _,
            inst,
            std::ptr::null(),
        );
        SendMessageW(c, WM_SETFONT, font as usize, 1);
        if class == "BUTTON" {
            crate::ui::interactive(c);
        }
        if class == "STATIC" {
            crate::ui::stable_label(c);
        }
        c
    };
    add("STATIC", "Diagnostics", 0, 24, 18, 800, 28, 0);
    add(
        "STATIC",
        "Copy this report when asking for help. Original videos are never included.",
        0,
        24,
        54,
        800,
        26,
        0,
    );
    let body = add(
        "EDIT",
        &report,
        404,
        24,
        94,
        802,
        314,
        ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32 | WS_VSCROLL | WS_TABSTOP,
    );
    SendMessageW(body, EM_SETSEL, 0, 0);
    add(
        "BUTTON",
        "Copy report",
        401,
        24,
        434,
        182,
        40,
        BS_OWNERDRAW as u32 | WS_TABSTOP,
    );
    add(
        "BUTTON",
        "Open raw log",
        402,
        222,
        434,
        182,
        40,
        BS_OWNERDRAW as u32 | WS_TABSTOP,
    );
    add(
        "BUTTON",
        "Close",
        403,
        644,
        434,
        182,
        40,
        BS_OWNERDRAW as u32 | WS_TABSTOP,
    );
    let notice = add("STATIC", "", 0, 420, 440, 208, 30, 0);
    EnableWindow(parent, 0);
    ShowWindow(h, SW_SHOW);
    SetForegroundWindow(h);
    let mut msg = MSG::default();
    loop {
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                PostQuitMessage(msg.wParam as i32);
                DestroyWindow(h);
                EnableWindow(parent, 1);
                return;
            }
            if msg.message == WM_KEYDOWN && msg.wParam == 27 {
                SetPropW(h, wide("action").as_ptr(), 403usize as _);
            } else if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let action = RemovePropW(h, wide("action").as_ptr()) as usize;
        match action {
            3 | 403 => break,
            401 => {
                SetWindowTextW(
                    notice,
                    wide(if copy(h, &report) {
                        "Report copied"
                    } else {
                        "Copy failed"
                    })
                    .as_ptr(),
                );
            }
            402 => {
                let _ = std::process::Command::new("notepad.exe").arg(&path).spawn();
            }
            _ => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    DestroyWindow(h);
    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
}
