//! Small modal "Update available" prompt with explicit button labels. Plain Win32 (no
//! TaskDialog, which needs a v6 common-controls manifest this exe does not have).
use std::sync::atomic::{AtomicI32, Ordering};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};
static CHOICE: AtomicI32 = AtomicI32::new(-1);
pub const UPDATE_NOW: i32 = 0;
pub const REMIND_LATER: i32 = 1;
pub const SKIP_VERSION: i32 = 2;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_COMMAND => {
            let id = (w & 0xffff) as i32;
            if (100..=102).contains(&id) {
                CHOICE.store(id - 100, Ordering::SeqCst);
            }
            0
        }
        // Closing the window is the harmless choice: ask again later.
        WM_CLOSE => {
            CHOICE.store(REMIND_LATER, Ordering::SeqCst);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
/// Blocks (nested loop, owner disabled) until the user picks. Returns UPDATE_NOW,
/// REMIND_LATER or SKIP_VERSION.
pub unsafe fn ask(owner: HWND, version: &str) -> i32 {
    let inst = GetModuleHandleW(std::ptr::null());
    let cls = wide("ZimClipUpdateAsk");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hbrBackground: (COLOR_BTNFACE + 1) as _,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    CHOICE.store(-1, Ordering::SeqCst);
    let (w, hh) = (440, 190);
    let mut o = RECT::default();
    GetWindowRect(owner, &mut o);
    let x = o.left + ((o.right - o.left) - w) / 2;
    let y = o.top + ((o.bottom - o.top) - hh) / 2;
    let title = wide("Update available");
    let dlg = CreateWindowExW(
        WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
        cls.as_ptr(),
        title.as_ptr(),
        WS_POPUP | WS_CAPTION | WS_SYSMENU,
        x,
        y,
        w,
        hh,
        owner,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    if dlg.is_null() {
        return REMIND_LATER;
    }
    let font = GetStockObject(DEFAULT_GUI_FONT);
    let mk = |class: &str, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: isize| {
        let c = wide(class);
        let t = wide(text);
        let c = CreateWindowExW(0, c.as_ptr(), t.as_ptr(), WS_CHILD | WS_VISIBLE | style, x, y, w, h, dlg, id as _, inst, std::ptr::null());
        SendMessageW(c, WM_SETFONT, font as usize, 1);
        c
    };
    mk(
        "STATIC",
        &format!("ZimClip {version} is available.\n\nUpdating downloads and verifies it, stops capture, clears the unsaved replay buffer and restarts the app. Saved clips and settings stay."),
        0,
        16,
        12,
        w - 48,
        90,
        0,
    );
    mk("BUTTON", "Update now", BS_DEFPUSHBUTTON as u32 | WS_TABSTOP, 16, 112, 120, 30, 100);
    mk("BUTTON", "Remind me later", BS_PUSHBUTTON as u32 | WS_TABSTOP, 146, 112, 130, 30, 101);
    mk("BUTTON", "Skip this version", BS_PUSHBUTTON as u32 | WS_TABSTOP, 286, 112, 124, 30, 102);
    EnableWindow(owner, 0);
    ShowWindow(dlg, SW_SHOW);
    let mut msg: MSG = std::mem::zeroed();
    while CHOICE.load(Ordering::SeqCst) < 0 {
        let r = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
        if r <= 0 {
            if r == 0 {
                PostQuitMessage(msg.wParam as i32);
            }
            CHOICE.store(REMIND_LATER, Ordering::SeqCst);
            break;
        }
        if IsDialogMessageW(dlg, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    EnableWindow(owner, 1);
    DestroyWindow(dlg);
    SetForegroundWindow(owner);
    CHOICE.load(Ordering::SeqCst)
}
