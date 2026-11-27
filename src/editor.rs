use std::sync::atomic::{AtomicBool, Ordering};
static OPEN: AtomicBool = AtomicBool::new(false);
static SAVING: AtomicBool = AtomicBool::new(false);
/// Switching the editor layout reopens the editor on the same clip with the new layout,
/// so both layouts always start from the same, fully built window. The trim range is carried.
static REOPEN: AtomicBool = AtomicBool::new(false);
// Video window repaint counters for the diagnostics log (how often the player was asked to redraw,
// and the last result).
static VIDEO_PAINTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static VIDEO_UPDATE_HR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
/// What a layout switch carries to the reopened editor: clip, trim range, playhead, window origin.
struct Carry {
    path: PathBuf,
    start: f64,
    end: f64,
    playhead: f64,
    mix: crate::mixer::AudioMix,
    x: i32,
    y: i32,
}
fn reopen_state() -> &'static std::sync::Mutex<Option<Carry>> {
    static S: std::sync::OnceLock<std::sync::Mutex<Option<Carry>>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}
fn saved_queue() -> &'static std::sync::Mutex<Vec<PathBuf>> {
    static Q: std::sync::OnceLock<std::sync::Mutex<Vec<PathBuf>>> = std::sync::OnceLock::new();
    Q.get_or_init(Default::default)
}
pub fn take_saved() -> Vec<PathBuf> {
    std::mem::take(&mut *saved_queue().lock().unwrap())
}

pub fn is_open() -> bool {
    OPEN.load(Ordering::SeqCst)
}
pub fn is_saving() -> bool {
    SAVING.load(Ordering::SeqCst)
}
use std::{path::PathBuf, sync::mpsc, time::Duration};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HWND as WHWND,
        Media::MediaFoundation::*,
        System::{Com::StructuredStorage::*, Com::*, Variant::VT_I8},
    },
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::*,
    UI::{
        Controls::*,
        Input::KeyboardAndMouse::{EnableWindow, GetCapture, GetFocus},
        WindowsAndMessaging::*,
    },
};
#[windows::core::implement(IMFPMediaPlayerCallback)]
struct PlaybackEvents {
    sender: mpsc::Sender<(i32, i32)>,
}
impl IMFPMediaPlayerCallback_Impl for PlaybackEvents_Impl {
    fn OnMediaPlayerEvent(&self, event: *const MFP_EVENT_HEADER) {
        unsafe {
            if let Some(e) = event.as_ref() {
                let _ = self.sender.send((e.eEventType.0, e.hrEvent.0));
            }
        }
    }
}
/// One native player with its own callback channel. A replaced session's callbacks can no
/// longer reach the editor: the old player is shut down and its sender is detached.
unsafe fn start_player(
    video: HWND,
    events: &mut crate::media::PlaybackSessions,
) -> (Option<IMFPMediaPlayer>, windows::core::Result<()>) {
    let callback: IMFPMediaPlayerCallback = PlaybackEvents {
        sender: events.fresh(),
    }
    .into();
    let mut player = None;
    let init = MFPCreateMediaPlayer(
        PCWSTR::null(),
        false,
        MFP_OPTION_FREE_THREADED_CALLBACK,
        &callback,
        Some(WHWND(video)),
        Some(&mut player),
    );
    (player, init)
}
fn note(s: &str) {
    use std::io::Write;
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let ms = T0.get_or_init(std::time::Instant::now).elapsed().as_millis();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            let file = root.join("editor-diagnostics.txt");
            // Keep the log bounded: past 1 MB the current file becomes the single .old backup.
            if std::fs::metadata(&file).map(|m| m.len() > 1_048_576).unwrap_or(false) {
                let _ = std::fs::rename(&file, root.join("editor-diagnostics.old.txt"));
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
                writeln!(f, "[+{ms}ms] {s}").ok();
            }
        }
    }
}
/// One line about the player and the video window for the diagnostics log. No paths.
unsafe fn video_state(p: &IMFPMediaPlayer, video: HWND) -> String {
    let state = p.GetState().map(|v| v.0).ok();
    let (mut has, mut selected) = (windows::core::BOOL(0), windows::core::BOOL(0));
    let has_video = match p.GetMediaItem() {
        Ok(item) => item
            .HasVideo(Some(&mut has), Some(&mut selected))
            .map(|_| (has.as_bool(), selected.as_bool()))
            .ok(),
        Err(_) => None,
    };
    let mut size = windows::Win32::Foundation::SIZE::default();
    let native = p.GetNativeVideoSize(Some(&mut size), None).map(|_| (size.cx, size.cy)).ok();
    let mut r = RECT::default();
    GetClientRect(video, &mut r);
    format!(
        "VIDEO state={state:?} has_video/selected={has_video:?} native={native:?} window_visible={} client={}x{} live_flag={} paints={} last_update_hr={:#x}",
        IsWindowVisible(video),
        r.right,
        r.bottom,
        !GetPropW(video, wide("preview-live").as_ptr()).is_null(),
        VIDEO_PAINTS.load(Ordering::Relaxed),
        VIDEO_UPDATE_HR.load(Ordering::Relaxed)
    )
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn text(h: HWND, s: &str) {
    let mut old = vec![0u16; (GetWindowTextLengthW(h) + 1).max(1) as usize];
    let n = GetWindowTextW(h, old.as_mut_ptr(), old.len() as i32);
    if String::from_utf16_lossy(&old[..n as usize]) != s {
        SetWindowTextW(h, wide(s).as_ptr());
    }
}
unsafe fn child(
    parent: HWND,
    class: &str,
    s: &str,
    id: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font: HFONT,
) -> HWND {
    let style = WS_CHILD
        | WS_VISIBLE
        | if class == "BUTTON" {
            WS_TABSTOP | BS_OWNERDRAW as u32
        } else if class == "COMBOBOX" {
            WS_TABSTOP
                | WS_VSCROLL
                | CBS_DROPDOWNLIST as u32
                | CBS_OWNERDRAWFIXED as u32
                | CBS_HASSTRINGS as u32
        } else if class == "msctls_trackbar32" {
            WS_TABSTOP
        } else {
            0
        };
    let mut parentclass = [0u16; 64];
    let n = GetClassNameW(parent, parentclass.as_mut_ptr(), 64);
    let workbench = String::from_utf16_lossy(&parentclass[..n as usize]) == "ZimClipEditor";
    let (x, y, w, h) = if workbench {
        match id {
            201 => (24, 414, 48, 36),
            207 => (936, 20, 40, 36),
            215 => (716, 20, 160, 36),
            213 => (888, 20, 36, 36),
            214 => (988, 20, 44, 36),
            204 => (24, 688, 38, 36),
            205 => (74, 688, 38, 36),
            209 => (994, 688, 38, 36),
            216 => (850, 414, 182, 36),
            206 => (748, 750, 284, 48),
            208 => (24, 754, 420, 280),
            210 => (548, 804, 180, 30),
            212 => (514, 762, 34, 24),
            _ => {
                if class == "ZimClipVideoHost" {
                    (24, 92, 1008, 310)
                } else {
                    match y {
                        14 => (136, 50, 510, 22),
                        44 => (136, 20, 510, 28),
                        70 => (24, 70, 900, 20),
                        346 => (84, 422, 400, 24),
                        646 => (132, 696, 800, 26),
                        742 => (24, 812, 1008, 38),
                        572 => (24, 656, 1008, 22),
                        _ => (x, y, w, h),
                    }
                }
            }
        }
    } else {
        (x, y, w, h)
    };
    let c = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(s).as_ptr(),
        style,
        x,
        y,
        w,
        h,
        parent,
        id as _,
        std::ptr::null_mut(),
        std::ptr::null(),
    );
    if class == "STATIC" {
        crate::ui::stable_label(c);
        if workbench {
            SetPropW(c, wide("footer-background").as_ptr(), 1usize as _);
            if matches!(y, 422 | 696) {
                SetPropW(c, wide("editor-readout").as_ptr(), 1usize as _);
            }
            if y == 656 {
                SetPropW(c, wide("editor-keycaps").as_ptr(), 1usize as _);
            }
        }
    }
    if class == "BUTTON" {
        crate::ui::interactive(c);
    }
    SendMessageW(c, WM_SETFONT, font as usize, 1);
    c
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_MOUSEWHEEL => {
            editor_scroll(h, if (w >> 16) as i16 > 0 { -80 } else { 80 });
            0
        }
        WM_VSCROLL => {
            match w & 65535 {
                0 => editor_scroll(h, -40),
                1 => editor_scroll(h, 40),
                2 => editor_scroll(h, -300),
                3 => editor_scroll(h, 300),
                6 => editor_scroll(h, -2000),
                7 => editor_scroll(h, 2000),
                4 | 5 => {
                    let mut i = SCROLLINFO {
                        cbSize: std::mem::size_of::<SCROLLINFO>() as _,
                        fMask: SIF_TRACKPOS,
                        ..std::mem::zeroed()
                    };
                    GetScrollInfo(h, SB_VERT, &mut i);
                    editor_scroll(
                        h,
                        i.nTrackPos - GetPropW(h, wide("editor-scroll").as_ptr()) as i32,
                    );
                }
                _ => {}
            }
            0
        }
        WM_COMMAND if w & 65535 == 217 => {
            if !SAVING.load(Ordering::SeqCst) {
                DestroyWindow(h);
            }
            0
        }
        WM_COMMAND | WM_HSCROLL => {
            use windows_sys::Win32::System::Threading::GetCurrentThreadId;
            PostThreadMessageW(
                GetCurrentThreadId(),
                if m == WM_COMMAND {
                    WM_APP + 51
                } else {
                    WM_APP + 52
                },
                w,
                l,
            );
            0
        }

        WM_CLOSE => {
            if SAVING.load(Ordering::SeqCst) {
                return 0;
            }
            DestroyWindow(h);
            0
        }
        WM_DESTROY => {
            crate::analysis::cancel_all();
            PostQuitMessage(0);
            0
        }
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let (dc, buffer) = crate::ui::begin_buffer(h, &mut ps);
            let mut r = std::mem::zeroed();
            GetClientRect(h, &mut r);
            let t = crate::ui::style_theme();
            FillRect(dc, &r, crate::ui::style::brush(t.bg));
            let offset = GetPropW(h, wide("editor-scroll").as_ptr()) as i32;
            SetViewportOrgEx(dc, 0, -offset, std::ptr::null_mut());
            let layout2 = GetPropW(h, wide("editor-layout").as_ptr()) as usize == 1;
            let heading = crate::ui::style::font(&crate::display::font_name(), 15, 600);
            let lane_top = if layout2 { 498 } else { 466 };
            if layout2 {
                for r in [
                    RECT { left: 16, top: 86, right: 804, bottom: 432 },
                    RECT { left: 16, top: 486, right: 804, bottom: 688 },
                    RECT { left: 824, top: 16, right: 1252, bottom: 280 },
                    RECT { left: 824, top: 292, right: 1252, bottom: 408 },
                ] {
                    crate::ui::style::rect(dc, r, t.panel, t.border, 14);
                }
                if IsWindowVisible(GetDlgItem(h, 211)) != 0 {
                    crate::ui::style::label(
                        dc,
                        "MB",
                        RECT { left: 922, top: 118, right: 970, bottom: 142 },
                        crate::ui::style::font(&crate::display::font_name(), 14, 400),
                        t.muted,
                        DT_SINGLELINE | DT_VCENTER,
                    );
                }
                if windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(GetDlgItem(h, 215)) == 0 {
                    crate::ui::style::label(
                        dc,
                        "Needs a ZimClip clip with Mix, System, Mic and Comms tracks",
                        RECT { left: 844, top: 378, right: 1232, bottom: 398 },
                        crate::ui::style::font(&crate::display::font_name(), 12, 400),
                        t.muted,
                        DT_SINGLELINE | DT_VCENTER,
                    );
                }
                for (label, y) in [("Export", 30), ("Audio tracks", 306)] {
                    crate::ui::style::label(
                        dc,
                        label,
                        RECT { left: 844, top: y, right: 1232, bottom: y + 28 },
                        heading,
                        t.text,
                        DT_SINGLELINE | DT_VCENTER,
                    );
                }
            } else {
                for r in [
                    RECT { left: 16, top: 86, right: 1040, bottom: 404 },
                    RECT { left: 16, top: 458, right: 1040, bottom: 650 },
                ] {
                    crate::ui::style::rect(dc, r, t.panel, t.border, 14);
                }
                crate::ui::style::rect(
                    dc,
                    RECT { left: 0, top: 738, right: 1056, bottom: 739 },
                    t.border,
                    t.border,
                    0,
                );
            }
            for (i, (name, color)) in [("System", 0x78df9a), ("Mic", 0xe8bf74), ("Comms", 0xe79db5)]
                .iter()
                .enumerate()
            {
                crate::ui::style::label(
                    dc,
                    name,
                    RECT {
                        left: 42,
                        top: lane_top + 58 + i as i32 * 36,
                        right: 108,
                        bottom: lane_top + 94 + i as i32 * 36,
                    },
                    crate::ui::style::font(&crate::display::font_name(), 12, 600),
                    *color,
                    DT_SINGLELINE | DT_VCENTER,
                );
            }
            for (i, color) in [0x78df9a, 0xe8bf74, 0xe79db5].iter().enumerate() {
                let y = lane_top + 73 + i as i32 * 36;
                crate::ui::style::disc(dc, RECT { left: 30, top: y, right: 37, bottom: y + 7 }, *color, None);
            }
            let progress = GetPropW(h, wide("export-progress").as_ptr()) as usize;
            if progress > 0 {
                let (px, py, pw) = if layout2 { (844, 270, 388) } else { (20, 802, 1012) };
                FillRect(
                    dc,
                    &RECT {
                        left: px,
                        top: py,
                        right: px + ((progress - 1).min(100) as i32 * pw / 100),
                        bottom: py + 4,
                    },
                    crate::ui::style::brush(t.accent),
                );
            }
            SetViewportOrgEx(dc, 0, 0, std::ptr::null_mut());
            // Slim themed overflow indicator, only when the content is taller than the window.
            if let Some(th) = editor_thumb(h) {
                let c = crate::ui::style::mix(t.panel, t.text, 30);
                crate::ui::style::rect(dc, th, c, c, 3);
            }
            crate::ui::end_buffer(h, &ps, dc, buffer);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            if editor_thumb(h).is_some() && (l as i16 as i32) >= r.right - 24 {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetCapture(h);
                editor_scroll_to(h, (l >> 16) as i16 as i32);
            }
            0
        }
        WM_MOUSEMOVE => {
            if GetCapture() == h && w & 1 != 0 {
                editor_scroll_to(h, (l >> 16) as i16 as i32);
            }
            0
        }
        WM_LBUTTONUP => {
            if GetCapture() == h {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(l as HWND, &mut r);
            let mut pt = POINT {
                x: r.left,
                y: r.top,
            };
            ScreenToClient(h, &mut pt);
            let mut own = RECT::default();
            GetWindowRect(l as HWND, &mut own);
            let mut origin = POINT { x: 0, y: 0 };
            ClientToScreen(l as HWND, &mut origin);
            let sample_y = pt.y + (origin.y - own.top);
            let color = if (458..650).contains(&sample_y) {
                crate::ui::panel()
            } else if pt.y >= 94 && pt.y < 404 {
                0x0b0b0b
            } else {
                crate::ui::bg()
            };
            SetBkColor(w as _, color);
            SetTextColor(w as _, crate::ui::text_color());
            SetBkMode(w as _, OPAQUE as _);
            static BRUSHES: std::sync::OnceLock<
                std::sync::Mutex<std::collections::BTreeMap<u32, usize>>,
            > = std::sync::OnceLock::new();
            let mut brushes = BRUSHES.get_or_init(Default::default).lock().unwrap();
            *brushes
                .entry(color)
                .or_insert_with(|| CreateSolidBrush(color) as usize) as isize
        }
        WM_DRAWITEM | WM_MEASUREITEM | WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT => {
            crate::ui::handle(h, m, w, l).unwrap_or(0)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
// MFPlay must receive the intended stream selection before SetMediaItem.
unsafe fn prepare_playback(item: &IMFPMediaItem, path: &std::path::Path) {
    let count = match item.GetNumberOfStreams() {
        Ok(n) => n,
        Err(e) => {
            note(&format!("Playback stream count unavailable: {e}"));
            return;
        }
    };
    if let Some(kinds) = crate::media::track_kinds(path).filter(|k| k.len() == count as usize) {
        for (index, select) in crate::media::audio_selection(&kinds)
            .into_iter()
            .enumerate()
        {
            if let Some(select) = select {
                let before = item.GetStreamSelection(index as u32);
                let r = item.SetStreamSelection(index as u32, select);
                note(&format!("Playback MP4 audio stream={index} before={before:?} select={select} result={r:?}"));
            }
        }
        return;
    }
    let mut first_audio = false;
    for index in 0..count {
        let before = item.GetStreamSelection(index);
        match item.GetStreamAttribute(index, &MF_MT_MAJOR_TYPE) {
            Ok(mut v) => {
                let kind = PropVariantToGUID(&v);
                let _ = PropVariantClear(&mut v);
                if kind == Ok(MFMediaType_Audio) {
                    let select = !first_audio;
                    first_audio = true;
                    let r = item.SetStreamSelection(index, select);
                    note(&format!("Playback audio stream={index} before={before:?} select={select} result={r:?}"));
                } else {
                    note(&format!(
                        "Playback stream={index} type={kind:?} selection={before:?}"
                    ));
                }
            }
            Err(e) => note(&format!(
                "Playback stream={index} type unavailable: {e}; selection={before:?}"
            )),
        }
    }
    let mut has = windows::core::BOOL(0);
    let mut selected = windows::core::BOOL(0);
    let r = item.HasAudio(Some(&mut has), Some(&mut selected));
    note(&format!(
        "Playback audio detected={} selected={} result={r:?}",
        has.as_bool(),
        selected.as_bool()
    ));
}
unsafe fn playback_bounds(
    p: &IMFPMediaPlayer,
    enabled: bool,
    _start: f64,
    end: f64,
) -> windows::core::Result<()> {
    let item = p.GetMediaItem()?;
    let a = PROPVARIANT::default();
    let mut b = PROPVARIANT::default();
    // Source MP4 mvhd duration and the freshly encoded audition's native
    // presentation duration can differ. Never set an out-of-range stop.
    let native_duration = item
        .GetDuration(&MFP_POSITIONTYPE_100NS)
        .ok()
        .and_then(|mut v| {
            let ticks = PropVariantToInt64(&v).ok();
            let _ = PropVariantClear(&mut v);
            ticks
        });
    if let Some(stop) = crate::media::preview_stop_ticks(enabled, end, native_duration) {
        (*b.Anonymous.Anonymous).vt = VT_I8;
        (*b.Anonymous.Anonymous).Anonymous.hVal = stop;
    }
    note(&format!(
        "Native preview bounds duration_ticks={native_duration:?} end={end:.7} enabled={enabled}"
    ));
    item.SetStartStopPosition(
        Some(&MFP_POSITIONTYPE_100NS),
        Some(&a),
        Some(&MFP_POSITIONTYPE_100NS),
        Some(&b),
    )?;
    p.SetMediaItem(&item)
}
unsafe fn seek_result(player: &IMFPMediaPlayer, value: f64) -> windows::core::Result<()> {
    let mut v = PROPVARIANT::default();
    (*v.Anonymous.Anonymous).vt = VT_I8;
    (*v.Anonymous.Anonymous).Anonymous.hVal = (value * 10_000_000.0) as i64;
    player.SetPosition(&MFP_POSITIONTYPE_100NS, &v)
}
unsafe fn position(player: &IMFPMediaPlayer, value: f64) {
    let _ = seek_result(player, value);
}
unsafe fn seconds(player: &IMFPMediaPlayer, duration: bool) -> Option<f64> {
    let mut v = if duration {
        player.GetDuration(&MFP_POSITIONTYPE_100NS)
    } else {
        player.GetPosition(&MFP_POSITIONTYPE_100NS)
    }
    .ok()?;
    let n = PropVariantToInt64(&v).ok();
    let _ = PropVariantClear(&mut v);
    n.map(|v| v as f64 / 10_000_000.0)
}
pub fn open(input: PathBuf, engine: PathBuf) {
    if OPEN.swap(true, Ordering::SeqCst) {
        unsafe {
            let class = wide("ZimClipEditor");
            let h = FindWindowW(class.as_ptr(), std::ptr::null());
            if !h.is_null() {
                ShowWindow(h, SW_RESTORE);
                SetForegroundWindow(h);
            }
        }
        return;
    }
    std::thread::spawn(move || unsafe {
        let mut input = input;
        loop {
            run(input.clone(), engine.clone());
            if !REOPEN.swap(false, Ordering::SeqCst) {
                break;
            }
            match reopen_state().lock().unwrap().as_ref() {
                Some(c) => input = c.path.clone(),
                None => break,
            }
        }
        OPEN.store(false, Ordering::SeqCst);
    });
}
unsafe fn run(mut input: PathBuf, engine: PathBuf) {
    // The editor runs its own UI thread, so it needs its own Ctrl+A hook for its edit fields.
    crate::ui::install_select_all();
    note("Workbench: opening local MP4");
    if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
        note("COM initialization failed");
        return;
    }
    let hinst = GetModuleHandleW(std::ptr::null());
    let class = wide("ZimClipEditor");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: hinst,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hIcon: LoadIconW(hinst, 1usize as _),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let h = CreateWindowExW(
        0,
        class.as_ptr(),
        wide("ZimClip | Replay workbench").as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        1088,
        editor_window_height(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        hinst,
        std::ptr::null(),
    );
    if h.is_null() {
        CoUninitialize();
        return;
    }
    {
        // Keep the whole window on the visible work area (default placement can push the
        // bottom edge off a small screen). The page scrolls when the client is shorter.
        let mut work = RECT::default();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as _, 0);
        let mut win = RECT::default();
        GetWindowRect(h, &mut win);
        let (ww, wh) = (win.right - win.left, win.bottom - win.top);
        // After a layout switch the editor comes back where it was.
        let (want_x, want_y) = match reopen_state().lock().unwrap().as_ref() {
            Some(c) => (c.x, c.y),
            None => (win.left, win.top),
        };
        let x = want_x.clamp(work.left, (work.right - ww).max(work.left));
        let y = want_y.clamp(work.top, (work.bottom - wh).max(work.top));
        if x != win.left || y != win.top {
            SetWindowPos(h, std::ptr::null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
    crate::ui::titlebar(h);
    crate::ui::apply_identity(h, true);
    let font = CreateFontW(
        -15,
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
    let headingfont = CreateFontW(
        -20,
        0,
        0,
        0,
        600,
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
    let metadata_label = child(
        h,
        "STATIC",
        "Reading clip metadata...",
        0,
        24,
        14,
        440,
        30,
        font,
    );
    let filenamelabel = child(
        h,
        "STATIC",
        &input.file_name().unwrap().to_string_lossy(),
        0,
        24,
        44,
        620,
        24,
        font,
    );
    let librarybutton = child(h, "BUTTON", "Library", 217, 24, 20, 98, 36, font);
    SetPropW(librarybutton, wide("editor-redesign").as_ptr(), 1usize as _);
    let layoutbutton = child(h, "BUTTON", "Editor layout", 218, 666, 20, 40, 36, font);
    SetPropW(layoutbutton, wide("editor-redesign").as_ptr(), 1usize as _);
    crate::ui::tooltip(layoutbutton, "Switch editor layout");
    crate::ui::tooltip(librarybutton, "Back to clip library");
    SendMessageW(filenamelabel, WM_SETFONT, headingfont as usize, 0);
    let videoclass = wide("ZimClipVideoHost");
    let vwc = WNDCLASSW {
        lpfnWndProc: Some(video_proc),
        hInstance: hinst,
        lpszClassName: videoclass.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&vwc);
    let video = child(h, "ZimClipVideoHost", "", 0, 24, 100, 936, 226, font);
    SetWindowLongPtrW(
        video,
        GWL_STYLE,
        (GetWindowLongPtrW(video, GWL_STYLE) as u32 | WS_CLIPSIBLINGS) as isize,
    );
    let mixerbutton = child(h, "BUTTON", "Audio mixer", 215, 650, 22, 150, 38, font);
    let separated = crate::media::has_separate_audio(&input);
    EnableWindow(mixerbutton, if separated { 1 } else { 0 });
    crate::ui::tooltip(
        mixerbutton,
        if separated {
            "Adjust system / mic / comms in this edit"
        } else {
            "This clip has no separate ZimClip audio tracks"
        },
    );
    let renamebutton = child(h, "BUTTON", "Rename", 213, 812, 22, 64, 38, font);
    let deletebutton = child(h, "BUTTON", "Delete", 214, 888, 22, 72, 38, font);
    crate::ui::tooltip(renamebutton, "Rename this replay");
    crate::ui::tooltip(deletebutton, "Delete this replay");
    let playbutton = child(h, "BUTTON", "Play / pause", 201, 24, 340, 48, 36, font);
    let external = child(h, "BUTTON", "Open externally", 207, 912, 340, 48, 36, font);
    crate::ui::tooltip(playbutton, "Play / pause (Space)");
    crate::ui::tooltip(external, "Open in your video player");
    let clock = child(h, "STATIC", "Loading replay...", 0, 90, 346, 580, 26, font);
    SetPropW(clock, wide("footer-background").as_ptr(), 1usize as _);
    let slider = child(h, "msctls_trackbar32", "", 203, 24, 606, 936, 30, font);
    SetWindowLongPtrW(
        slider,
        GWL_STYLE,
        (GetWindowLongPtrW(slider, GWL_STYLE) as u32 | TBS_ENABLESELRANGE) as isize,
    );
    SendMessageW(slider, TBM_SETRANGEMAX, 1, 1000);
    ShowWindow(slider, SW_HIDE);
    let trimstart = child(h, "BUTTON", "Set start", 204, 24, 604, 112, 36, font);
    let trimend = child(h, "BUTTON", "Set end", 205, 144, 604, 112, 36, font);
    crate::ui::tooltip(trimstart, "Set trim start at playhead");
    crate::ui::tooltip(trimend, "Set trim end at playhead");
    let range = child(h, "STATIC", "Range: loading", 0, 24, 646, 740, 26, font);
    child(
        h,
        "BUTTON",
        "Save trimmed copy",
        206,
        676,
        686,
        284,
        44,
        font,
    );
    let status = child(
        h,
        "STATIC",
        "Original stays untouched. Choose Fast for keyframe cuts or Precise for exact trim points.",
        0,
        24,
        742,
        936,
        48,
        font,
    );
    let preset = child(h, "COMBOBOX", "", 208, 24, 688, 440, 220, font);

    for s in [
        "Fast trim  /  original quality, keyframe cuts",
        "Precise trim  /  source size, 6 Mbps",
        "Share 720p  /  1.8 Mbps",
        "Small 480p  /  0.85 Mbps",
        "Under 10 MB / keep resolution",
        "Under 20 MB / keep resolution",
        "Size limit MB / keep resolution",
    ] {
        SendMessageW(preset, CB_ADDSTRING, 0, wide(s).as_ptr() as isize);
    }
    SendMessageW(preset, CB_SETCURSEL, 0, 0);
    crate::ui::dropdown(preset);
    let targetinput = CreateWindowExW(
        0,
        wide("EDIT").as_ptr(),
        wide("10").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_NUMBER as u32 | ES_CENTER as u32,
        454,
        758,
        60,
        24,
        h,
        211usize as _,
        hinst,
        std::ptr::null(),
    );
    SendMessageW(targetinput, WM_SETFONT, font as usize, 1);
    SendMessageW(targetinput, EM_SETLIMITTEXT, 3, 0);
    ShowWindow(targetinput, SW_HIDE);
    crate::ui::clip_round(targetinput, 12);
    crate::ui::tooltip(
        targetinput,
        "Upper size limit, 1 to 500 MB. Output may be smaller; no padding.",
    );
    let mblabel = child(h, "STATIC", "MB", 212, 554, 698, 32, 24, font);
    ShowWindow(mblabel, SW_HIDE);
    // Setup "Share size" default: only preselects the export size, nothing is exported by it.
    if let Some(mb) = crate::file_prefs::prefs_file()
        .parent()
        .and_then(crate::file_prefs::share_default_mb)
    {
        let index = match mb {
            10 => 4,
            20 => 5,
            _ => 6,
        };
        SendMessageW(preset, CB_SETCURSEL, index, 0);
        if index == 6 {
            SetWindowTextW(targetinput, wide(&mb.to_string()).as_ptr());
            ShowWindow(targetinput, SW_SHOW);
            ShowWindow(mblabel, SW_SHOW);
        }
    }
    let resetbutton = child(h, "BUTTON", "Reset range", 209, 264, 604, 112, 36, font);
    crate::ui::tooltip(resetbutton, "Reset to full replay");
    let cancelbutton = child(h, "BUTTON", "Cancel export", 210, 790, 644, 170, 34, font);
    crate::ui::tooltip(cancelbutton, "Cancel export");
    EnableWindow(cancelbutton, 0);
    ShowWindow(cancelbutton, SW_HIDE);
    let timelineclass = wide("ZimClipTimeline");
    let twc = WNDCLASSW {
        lpfnWndProc: Some(timeline_proc),
        hInstance: hinst,
        lpszClassName: timelineclass.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
        ..std::mem::zeroed()
    };
    RegisterClassW(&twc);
    let timeline = CreateWindowExW(
        0,
        timelineclass.as_ptr(),
        wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        112,
        466,
        920,
        178,
        h,
        std::ptr::null_mut(),
        hinst,
        std::ptr::null(),
    );
    SendMessageW(timeline, WM_SETFONT, font as usize, 1);
    crate::ui::clip_round(timeline, 16);
    for id in [201, 204, 205, 206, 207, 209, 210, 213, 214, 215] {
        SetPropW(
            GetDlgItem(h, id),
            wide("editor-redesign").as_ptr(),
            1usize as _,
        );
    }
    let (analysis_tx, analysis_rx) = mpsc::channel();
    let helper = engine.parent().unwrap().join("media-analysis.exe");
    let analysis_input = input.clone();
    let analysis_duration = crate::media::duration(&input).unwrap_or(30.0);
    std::thread::spawn(move || {
        let began = std::time::Instant::now();
        let r = crate::analysis::load(&helper, &analysis_input, analysis_duration, &analysis_tx);
        note(&format!(
            "Timeline analysis completed ms={}",
            began.elapsed().as_millis()
        ));
        analysis_tx.send(r).ok();
    });
    let mut playback_events = crate::media::PlaybackSessions::default();
    let mut media_path = input.clone();
    let mut binding = false;
    let mut space_notes = 0u32;
    let mut wait_since: Option<std::time::Instant> = None;
    let mut pending_play: Option<f64> = None;
    let mut pending_position: Option<f64> = None;
    let carried = reopen_state().lock().unwrap().take();
    let mut play_after_position = false;
    let mut bounds_active = false;
    let (mut player, init) = start_player(video, &mut playback_events);
    note(&format!("MFPCreateMediaPlayer={init:?}"));
    if let Some(p) = &player {
        SetPropW(
            video,
            wide("native-player").as_ptr(),
            Box::into_raw(Box::new(p.clone())) as _,
        );
    }
    let mut duration = crate::media::duration(&input).unwrap_or(0.0);
    note(&format!("MP4 duration={duration:.3}s"));
    let bytes = std::fs::metadata(&input).map(|m| m.len()).ok();
    let duration_text = if duration > 0.0 {
        format!("{duration:.1} s")
    } else {
        "Duration unavailable".into()
    };
    text(
        metadata_label,
        &format!(
            "{duration_text}  /  {}",
            bytes
                .map(|b| format!("{:.1} MB", b as f64 / 1_000_000.0))
                .unwrap_or_else(|| "Size unavailable".into())
        ),
    );
    if init.is_ok() && !(separated && duration > 0.0 && duration <= 301.0) {
        if let Some(p) = &player {
            let mut item = None;
            let path = wide(&input.to_string_lossy());
            let loaded = p.CreateMediaItemFromURL(PCWSTR(path.as_ptr()), true, 0, Some(&mut item));
            note(&format!("CreateMediaItem={loaded:?}"));
            if loaded.is_ok() {
                if let Some(item) = item {
                    prepare_playback(&item, &input);
                    let set = p.SetMediaItem(&item);
                    binding = set.is_ok();
                    note(&format!("SetMediaItem={set:?}"));
                    if set.is_ok() {
                        duration = seconds(p, true).unwrap_or(duration);
                    }
                }
            }
        }
    }
    let mut preview_ok = player.as_ref().is_some_and(|p| p.GetMediaItem().is_ok());
    note(&format!("Preview media item ready={preview_ok}"));
    let mut preview_reported = preview_ok;
    // The host lets the native player paint only once a media item is set; until then it paints
    // its own background and the unavailable text, whatever the first paint time is.
    if preview_ok {
        SetPropW(video, wide("preview-live").as_ptr(), 1usize as _);
    }
    let mut start = 0.0;
    let mut end = duration;
    // After a layout switch the editor reopens on the same clip; restore its trim range.
    let mut current: f64 = 0.0;
    if let Some(c) = &carried {
        if c.path == input && duration > 0.0 {
            start = c.start.clamp(0.0, duration);
            end = c.end.clamp(start, duration);
            current = c.playhead.clamp(start, end);
            // The player seeks here once its media item is set (paused).
            if current > 0.0 {
                pending_position = Some(current);
            }
        }
    }
    let (tx, rx) = mpsc::channel::<Result<(PathBuf, Option<String>), String>>();
    let mut saving = false;
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let (progress_tx, progress_rx) = mpsc::channel();
    let mut selected_mode = crate::export::Mode::Fast;
    let mut audio_mix = crate::mixer::AudioMix::default();
    let start_mix = crate::mixer::start_mix(carried.as_ref().map(|c| (c.path.as_path(), c.mix)), &input);
    let mut mix_changed = false;
    let mut pending_mix = None;
    let (mix_tx, mix_rx) = mpsc::channel::<Result<PathBuf, String>>();
    let mut mixing = false;
    let mix_cancel = std::sync::Arc::new(AtomicBool::new(false));
    let mut preview_files = Vec::new();
    let mut pending_seek: Option<f64> = None;
    let mut last_seek = std::time::Instant::now() - Duration::from_secs(2);
    let mut user_seek = std::time::Instant::now() - Duration::from_secs(2);
    let mut dragging = false;
    let preview_preference = engine.parent().unwrap().join("preview-trim-only.txt");
    let mut trim_only = std::fs::read_to_string(&preview_preference)
        .map(|s| s.trim() != "0")
        .unwrap_or(true);
    let previewtoggle = child(
        h,
        "BUTTON",
        if trim_only {
            "Trim preview: ON"
        } else {
            "Trim preview: OFF"
        },
        216,
        398,
        604,
        210,
        36,
        font,
    );
    SetPropW(previewtoggle, wide("editor-redesign").as_ptr(), 1usize as _);
    crate::ui::tooltip(
        previewtoggle,
        "Restrict Play to trim points. Stops at the end; does not loop.",
    );
    let keycaps = child(
        h,
        "STATIC",
        "Drag handles to trim / Arrows 33ms / Shift 1s / [ ] set range",
        0,
        24,
        572,
        936,
        24,
        font,
    );
    let mut playing = false;
    let mut resume_after_drag = false;
    // A drag-time seek is in flight until the player reports it; the next one waits for it.
    let mut seek_inflight: Option<std::time::Instant> = None;
    // Trim bounds currently set on the loaded item: (trim preview on, stop time bits). None
    // whenever a new item was set, so a changed item or changed bounds always get a full setup.
    let mut bound_key: Option<(bool, u64, u64)> = None;
    let mut last_beat = std::time::Instant::now();

    EnableWindow(GetDlgItem(h, 206), if duration > 0.0 { 1 } else { 0 });
    let mut msg: MSG = std::mem::zeroed();
    let mut refresh = std::time::Instant::now();
    if !preview_ok {
        text(
            video,
            "Preview unavailable here. Use Open externally to check playback.",
        );
        text(
            status,
            "Preview unavailable. Timeline trim still works; open externally to check times.",
        );
    }
    EnumChildWindows(h, Some(editor_register_scroll), 0);
    let layout_file = engine.parent().unwrap().join("editor-layout.txt");
    let ctl = EdCtl {
        metadata: metadata_label,
        filename: filenamelabel,
        video,
        clock,
        range,
        status,
        timeline,
        keycaps,
        target: targetinput,
        mblabel,
    };
    let mut editor_layout: usize = std::fs::read_to_string(&layout_file)
        .map(|s| (s.trim() == "2") as usize)
        .unwrap_or(0);
    if editor_layout == 1 {
        editor_apply_layout(h, 1, &ctl);
    }
    editor_scroll(h, 0);
    // First paint happens after the saved layout is placed, so no classic-position pixels exist.
    ShowWindow(h, SW_SHOW);
    UpdateWindow(h);
    // Start with keyboard focus on the timeline so Space reaches Play from the first key.
    windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(timeline);
    {
        let f = GetFocus();
        let mut cls = [0u16; 32];
        let n = if f.is_null() { 0 } else { GetClassNameW(f, cls.as_mut_ptr(), 32) };
        note(&format!(
            "Initial focus: timeline_focused={} class={}",
            f == timeline,
            String::from_utf16_lossy(&cls[..n.max(0) as usize])
        ));
    }
    if duration <= 0.0 {
        text(status, "Cannot read replay duration. Trim disabled.");
    }
    let mut initial_mix = false;
    if separated && duration > 0.0 && duration <= 301.0 {
        initial_mix = true;
        mixing = true;
        SAVING.store(true, Ordering::SeqCst);
        pending_mix = Some(start_mix);
        EnableWindow(playbutton, 0);
        EnableWindow(GetDlgItem(h, 206), 0);
        text(mixerbutton, "Cancel preparation");
        text(
            status,
            "Preparing playback audio from System, Mic and Comms...",
        );
        let helper = engine.parent().unwrap().join("media-export.exe");
        let source = input.clone();
        let sender = mix_tx.clone();
        let cancellation = mix_cancel.clone();
        std::thread::spawn(move || {
            let began = std::time::Instant::now();
            let r = crate::mixer::preview(
                &helper,
                &source,
                start_mix,
                cancellation,
            );
            note(&format!(
                "Initial playback mix elapsed_ms={} success={}",
                began.elapsed().as_millis(),
                r.is_ok()
            ));
            let _ = sender.send(r);
        });
    }
    loop {
        while let Some((kind, hr)) = playback_events.try_recv() {
            note(&format!("Playback event kind={kind} hr={hr:#x} binding={binding} play_after_position={play_after_position}"));
            if kind == MFP_EVENT_TYPE_POSITION_SET.0 {
                seek_inflight = None;
            }
            if kind == MFP_EVENT_TYPE_PLAY.0 || kind == MFP_EVENT_TYPE_POSITION_SET.0 || kind == MFP_EVENT_TYPE_MEDIAITEM_SET.0 {
                if let Some(p) = &player {
                    note(&video_state(p, video));
                }
            }
            if kind == MFP_EVENT_TYPE_POSITION_SET.0 && play_after_position && hr != 1 {
                play_after_position = false;
                binding = false;
                if hr >= 0 {
                    if let Some(p) = &player {
                        let result = p.Play();
                        playing = result.is_ok();
                        note(&format!("Play after position event result={result:?}"));
                        if result.is_err() {
                            text(
                                status,
                                "Playback could not start. Open externally or check Diagnostics.",
                            );
                        }
                    }
                } else {
                    text(
                        status,
                        "Playback seek failed. Open externally or check Diagnostics.",
                    );
                }
            }
            if kind == MFP_EVENT_TYPE_MEDIAITEM_SET.0 && hr != 1 && binding {
                binding = false;
                note(&format!("Media item set event hr={hr:#x}"));
                if let Some(target) = pending_position.take() {
                    if hr >= 0 {
                        if let Some(p) = &player {
                            position(p, target);
                        }
                    }
                }
                if let Some(target) = pending_play.take() {
                    if hr >= 0 {
                        if let Some(p) = &player {
                            let result = seek_result(p, target);
                            note(&format!(
                                "Seek before Play target={target:.3} result={result:?}"
                            ));
                            if result.is_ok() {
                                play_after_position = true;
                            } else {
                                text(
                                    status,
                                    "Playback seek failed. Open externally or check Diagnostics.",
                                );
                            }
                        }
                    } else {
                        text(status,"Playback could not load this item. Open externally or check Diagnostics.");
                    }
                }
            }
        }
        if (binding || play_after_position) && !mixing {
            let since = *wait_since.get_or_insert_with(std::time::Instant::now);
            if crate::media::playback_wait_expired(since.elapsed()) {
                note(&format!(
                    "Playback wait expired binding={binding} play_after_position={play_after_position} pending_play={pending_play:?} pending_position={pending_position:?} pending_seek={pending_seek:?} bounds_active={bounds_active} playing={playing}"
                ));
                // New player session: shut the old player down and detach its callbacks.
                if let Some(old) = player.take() {
                    let _ = old.Shutdown();
                }
                let old_prop = RemovePropW(video, wide("native-player").as_ptr()) as *mut IMFPMediaPlayer;
                if !old_prop.is_null() {
                    drop(Box::from_raw(old_prop));
                }
                RemovePropW(video, wide("preview-live").as_ptr());
                let (fresh, init2) = start_player(video, &mut playback_events);
                note(&format!("Playback recovery MFPCreateMediaPlayer={init2:?}"));
                binding = false;
                play_after_position = false;
                pending_play = None;
                pending_position = None;
                pending_seek = None;
                resume_after_drag = false;
                bounds_active = false;
                bound_key = None;
                playing = false;
                preview_ok = false;
                preview_reported = false;
                wait_since = None;
                SetPropW(playbutton, wide("playing").as_ptr(), std::ptr::null_mut());
                InvalidateRect(playbutton, std::ptr::null(), 0);
                let mut reloaded = false;
                if let Some(p) = &fresh {
                    SetPropW(
                        video,
                        wide("native-player").as_ptr(),
                        Box::into_raw(Box::new(p.clone())) as _,
                    );
                    let mut item = None;
                    let name = wide(&media_path.to_string_lossy());
                    if p.CreateMediaItemFromURL(PCWSTR(name.as_ptr()), true, 0, Some(&mut item)).is_ok() {
                        if let Some(item) = item {
                            prepare_playback(&item, &media_path);
                            if p.SetMediaItem(&item).is_ok() {
                                binding = true;
                                pending_position = Some(current);
                                reloaded = true;
                            }
                        }
                    }
                }
                player = fresh;
                note(&format!("Playback recovery reload ok={reloaded}"));
                text(
                    status,
                    if reloaded {
                        "Preview was not ready and is reloading. Press Play when it is ready."
                    } else {
                        "Preview could not restart. Close and reopen the editor, or use Open externally."
                    },
                );
            }
        } else {
            wait_since = None;
        }
        if let Ok(result) = mix_rx.try_recv() {
            let was_initial = initial_mix;
            initial_mix = false;
            mixing = false;
            EnableWindow(
                playbutton,
                if was_initial && result.is_err() { 0 } else { 1 },
            );
            SAVING.store(false, Ordering::SeqCst);
            EnableWindow(mixerbutton, 1);
            text(mixerbutton, "Audio mixer");
            EnableWindow(GetDlgItem(h, 206), 1);
            match result {
                Ok(path) => {
                    if let Some(m) = pending_mix.take() {
                        audio_mix = m;
                        mix_changed = !was_initial;
                    }
                    note(&format!(
                        "Playback mix loaded clip={} gains={}/{}/{} initial={was_initial}",
                        input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                        audio_mix.system,
                        audio_mix.mic,
                        audio_mix.comms
                    ));
                    if let Some(p) = &player {
                        // Nothing to clear when no item was ever set (initial prep on a multi-track clip).
                        if preview_ok {
                            note("Reload: ClearMediaItem begin");
                            let _ = p.ClearMediaItem();
                            note("Reload: ClearMediaItem end");
                        }
                        let mut item = None;
                        let name = wide(&path.to_string_lossy());
                        note("Reload: CreateMediaItemFromURL begin");
                        let created = p.CreateMediaItemFromURL(PCWSTR(name.as_ptr()), true, 0, Some(&mut item));
                        note(&format!("Reload: CreateMediaItemFromURL end ok={}", created.is_ok()));
                        if created.is_ok()
                        {
                            if let Some(item) = item {
                                note("Reload: prepare_playback begin");
                                prepare_playback(&item, &path);
                                note("Reload: SetMediaItem begin");
                                let set = p.SetMediaItem(&item);
                                binding = set.is_ok();
                                if set.is_ok() {
                                    media_path = path.clone();
                                }
                                note(&format!("Reload SetMediaItem={set:?}"));
                                pending_position = Some(current);
                                bounds_active = false;
                                bound_key = None;
                            }
                        }
                    }
                    preview_files.push(path);
                    let audible = player.as_ref().is_some_and(|p| p.GetMediaItem().is_ok());
                    text(
                        status,
                        if audible {
                            if was_initial {
                                "Playback ready. Fresh default System/Mic/Comms mix; originals unchanged."
                            } else {
                                "Audition mix loaded. Export uses these gains; source tracks stay unchanged."
                            }
                        } else {
                            "Mix ready for export; preview unavailable here. Source tracks stay unchanged."
                        },
                    );
                }
                Err(e) => {
                    pending_mix = None;
                    // A layout switch carried a non-default mix; keep it for export even though
                    // the preview could not be built, and say so instead of claiming it played.
                    let kept = was_initial && start_mix != crate::mixer::AudioMix::default();
                    if kept {
                        audio_mix = start_mix;
                    }
                    text(
                        status,
                        &if kept {
                            {
                                note(&format!("Initial preview failed, carried mix kept for export: {e}"));
                                format!(
                                    "Preview failed. Your mix ({}/{}/{}) is kept for export, not previewed.",
                                    start_mix.system, start_mix.mic, start_mix.comms
                                )
                            }
                        } else if was_initial {
                            format!("Playback preparation failed: {e}. Try Apply mix or Open externally.")
                        } else {
                            e
                        },
                    );
                }
            }
        }
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_CLOSE && (saving || mixing) {
                text(
                    status,
                    if mixing {
                        "Playback preparation is running. Cancel preparation before closing."
                    } else {
                        "A trimmed copy is saving. Wait before closing."
                    },
                );
                continue;
            }
            if msg.message == WM_QUIT {
                if let Some(p) = &player {
                    let _ = p.Shutdown();
                }
                DeleteObject(font);
                DeleteObject(headingfont);
                for file in preview_files {
                    let parent = file.parent().map(|p| p.to_owned());
                    let _ = std::fs::remove_file(file);
                    if let Some(folder) = parent {
                        let _ = std::fs::remove_dir(folder);
                    }
                }
                CoUninitialize();
                return;
            }
            if msg.message == WM_KEYDOWN
                && GetFocus() != slider
                && GetFocus() != preset
                && GetFocus() != targetinput
                && (msg.wParam == 0x25 || msg.wParam == 0x27)
            {
                let step = if windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10) < 0
                {
                    1.0
                } else {
                    1.0 / 30.0
                };
                current =
                    (current + if msg.wParam == 0x25 { -step } else { step }).clamp(0.0, duration);
                pending_play = None;
                play_after_position = false;
                pending_seek = Some(current);
                user_seek = std::time::Instant::now();
            }
            if msg.message == WM_APP + 51 {
                note(&format!("EDITOR button id={}", msg.wParam & 0xffff));
                match msg.wParam & 0xffff {
                    208 => {
                        let custom = SendMessageW(preset, CB_GETCURSEL, 0, 0) == 6;
                        ShowWindow(targetinput, if custom { SW_SHOW } else { SW_HIDE });
                        ShowWindow(mblabel, if custom { SW_SHOW } else { SW_HIDE });
                        InvalidateRect(h, std::ptr::null(), 0);
                    }
                    218 if !saving && !mixing => {
                        let next = 1 - editor_layout;
                        match std::fs::write(&layout_file, if next == 1 { "2" } else { "1" }) {
                            Ok(()) => {
                                if let Some(p) = &player {
                                    let _ = p.Pause();
                                }
                                let mut win = RECT::default();
                                GetWindowRect(h, &mut win);
                                *reopen_state().lock().unwrap() = Some(Carry {
                                    path: input.clone(),
                                    start,
                                    end,
                                    playhead: current,
                                    mix: audio_mix,
                                    x: win.left,
                                    y: win.top,
                                });
                                REOPEN.store(true, Ordering::SeqCst);
                                DestroyWindow(h);
                            }
                            Err(e) => {
                                note(&format!("Editor layout preference not saved: {e}"));
                                text(status, "Could not save the layout choice, so the layout stays as it is.");
                            }
                        }
                    }
                    218 => {
                        text(status, "Finish the current export or mix before switching layout.");
                    }
                    215 if mixing => {
                        mix_cancel.store(true, Ordering::SeqCst);
                        text(status, "Cancelling audition mix...");
                    }
                    215 if !saving && !mixing && separated => {
                        if duration > 301.0 {
                            text(
                                status,
                                "Audio audition supports clips up to 300 seconds in this build.",
                            );
                            continue;
                        }
                        if let Some(m) = mixer_dialog(h, font, audio_mix) {
                            if let Some(p) = &player {
                                let _ = p.Pause();
                            }
                            playing = false;
                            pending_mix = Some(m);
                            mixing = true;
                            SAVING.store(true, Ordering::SeqCst);
                            mix_cancel.store(false, Ordering::SeqCst);
                            text(mixerbutton, "Cancel mix");
                            EnableWindow(GetDlgItem(h, 206), 0);
                            text(status,"Building audition mix. Original video and source tracks stay unchanged...");
                            let helper = engine.parent().unwrap().join("media-export.exe");
                            let i = input.clone();
                            let tx = mix_tx.clone();
                            let cancel = mix_cancel.clone();
                            std::thread::spawn(move || {
                                let _ = tx.send(crate::mixer::preview(&helper, &i, m, cancel));
                            });
                        }
                    }
                    207 => {
                        use windows_sys::Win32::UI::Shell::ShellExecuteW;
                        ShellExecuteW(
                            h,
                            wide("open").as_ptr(),
                            wide(&input.to_string_lossy()).as_ptr(),
                            std::ptr::null(),
                            std::ptr::null(),
                            SW_SHOWNORMAL,
                        );
                    }
                    213 | 214 if !saving && !mixing => {
                        let rename = msg.wParam & 0xffff == 213;
                        let prefs = engine.parent().unwrap().join("file-actions.ini");
                        let skip = crate::file_prefs::delete_confirm_skipped(&prefs);
                        let name = input.file_name().unwrap().to_string_lossy().to_string();
                        let response = if !rename && skip {
                            Some((String::new(), false))
                        } else {
                            file_dialog(h, font, rename, &name)
                        };
                        if let Some((newname, dontask)) = response {
                            let destination = if rename {
                                match crate::clip_files::valid_clip_name(&newname) {
                                    Ok(n) => input.with_file_name(n),
                                    Err(e) => {
                                        text(status, &e);
                                        continue;
                                    }
                                }
                            } else {
                                let trash = input.parent().unwrap().join("Trash");
                                if let Err(e) = std::fs::create_dir_all(&trash) {
                                    text(status, &format!("Cannot create Trash: {e}"));
                                    continue;
                                }
                                trash.join(format!(
                                    "{}-{}",
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_nanos(),
                                    name
                                ))
                            };
                            if destination == input {
                                continue;
                            }
                            if destination.exists() {
                                text(
                                    status,
                                    "A replay with that name already exists. Nothing changed.",
                                );
                                continue;
                            }
                            if let Some(p) = &player {
                                let _ = p.Pause();
                                let _ = p.ClearMediaItem();
                            }
                            playing = false;
                            SetPropW(playbutton, wide("playing").as_ptr(), std::ptr::null_mut());
                            InvalidateRect(playbutton, std::ptr::null(), 0);
                            if !rename {
                                if let Err(e) = crate::catalog::trash_metadata(&input, &destination)
                                {
                                    text(
                                        status,
                                        &format!("Clip unchanged, tags backup failed: {e}"),
                                    );
                                    continue;
                                }
                            }
                            let changed = std::fs::rename(&input, &destination);
                            if changed.is_ok() {
                                saved_queue().lock().unwrap().push(if rename {
                                    destination.clone()
                                } else {
                                    input.clone()
                                });
                                if !rename {
                                    if dontask {
                                        crate::file_prefs::set_delete_confirm_skipped(&prefs, true).ok();
                                    }
                                    note("Open replay moved to local Trash");
                                    DestroyWindow(h);
                                    continue;
                                }
                                let tag_error = crate::catalog::renamed(&input, &destination).err();
                                if let Some(e) = &tag_error {
                                    text(
                                        status,
                                        &format!("Renamed video, but tags could not follow: {e}"),
                                    );
                                }
                                input = destination;
                                text(filenamelabel, &input.file_name().unwrap().to_string_lossy());
                                if tag_error.is_none() {
                                    text(status, "Replay renamed. Other files are unchanged.");
                                }
                            } else {
                                text(status, &format!("File unchanged: {}", changed.unwrap_err()));
                            }
                            if let Some(p) = &player {
                                let mut item = None;
                                let playback_path = preview_files.last().unwrap_or(&input);
                                let path = wide(&playback_path.to_string_lossy());
                                if p.CreateMediaItemFromURL(
                                    PCWSTR(path.as_ptr()),
                                    true,
                                    0,
                                    Some(&mut item),
                                )
                                .is_ok()
                                {
                                    if let Some(item) = item {
                                        prepare_playback(&item, playback_path);
                                        let set = p.SetMediaItem(&item);
                                        binding = set.is_ok();
                                        note(&format!("Reload SetMediaItem={set:?}"));
                                        pending_position = Some(current);
                                        bounds_active = false;
                                        bound_key = None;
                                    }
                                }
                            }
                        }
                    }
                    216 => {
                        pending_play = None;
                        play_after_position = false;
                        resume_after_drag = false;
                        trim_only = !trim_only;
                        match std::fs::write(&preview_preference, if trim_only { "1" } else { "0" })
                        {
                            Ok(()) => {}
                            Err(e) => {
                                text(status, &format!("Preview choice active but not saved: {e}"))
                            }
                        };
                        text(
                            previewtoggle,
                            if trim_only {
                                "Trim preview: ON"
                            } else {
                                "Trim preview: OFF"
                            },
                        );
                        if playing {
                            if let Some(p) = &player {
                                let _ = p.Pause();
                                position(
                                    p,
                                    if trim_only {
                                        current.clamp(start, end)
                                    } else {
                                        current
                                    },
                                );
                            }
                            if trim_only {
                                current = current.clamp(start, end);
                            }
                            playing = false;
                        }
                    }
                    201 if !mixing => {
                        note(&format!(
                            "Play pressed: preview_ok={preview_ok} binding={binding} play_after_position={play_after_position} playing={playing} pending_seek={} state={:?}",
                            pending_seek.is_some(),
                            player.as_ref().and_then(|p| p.GetState().ok())
                        ));
                        if let Some(p) = &player {
                            if binding || play_after_position {
                                text(status, "Preparing preview. Try Play when it is ready.");
                                continue;
                            }
                            if playing {
                                let _ = p.Pause();
                                playing = false;
                            } else {
                                if trim_only && (current < start || current >= end - 0.04) {
                                    current = start;
                                }
                                pending_seek = None;
                                let want_key = if trim_only { (true, start.to_bits(), end.to_bits()) } else { (false, 0, 0) };
                                let paused = p.GetState().map(|v| v == MFP_MEDIAPLAYER_STATE_PAUSED).unwrap_or(false);
                                if crate::media::can_skip_rebind(paused, bound_key, want_key) {
                                    // Same item, same trim bounds, player paused: nothing to set up again.
                                    // Seek and play only (a changed item or bounds takes the full path below).
                                    note(&format!("Play from paused: item and bounds unchanged, no rebind. {}", video_state(p, video)));
                                    let seek = seek_result(p, current);
                                    play_after_position = seek.is_ok();
                                    if seek.is_err() {
                                        text(status, "Playback seek failed. Open externally or check Diagnostics.");
                                    }
                                } else {
                                let result = playback_bounds(p, trim_only, start, end);
                                note(&format!("Playback trim bounds enabled={trim_only} end={end:.3} result={result:?}"));
                                if result.is_ok() {
                                    binding = true;
                                    bounds_active = trim_only;
                                    bound_key = Some(want_key);
                                    pending_play = Some(current);
                                } else {
                                    bound_key = None;
                                    // Bounds are an optional playback aid. Keep preview usable
                                    // if the platform rejects them; the clock guard still stops
                                    // trim preview, with up to one refresh interval of tolerance.
                                    bounds_active = false;
                                    let seek = seek_result(p, current);
                                    play_after_position = seek.is_ok();
                                    text(
                                        status,
                                        if seek.is_ok() {
                                            "Preview ready with timed trim stop (up to 50 ms tolerance)."
                                        } else {
                                            "Preview seek failed. Use Open externally; see Diagnostics."
                                        },
                                    );
                                }
                                }
                            }
                            SetPropW(playbutton, wide("playing").as_ptr(), playing as usize as _);
                            InvalidateRect(playbutton, std::ptr::null(), 0);
                        }
                    }
                    201 => {
                        note("Play pressed while playback audio is still preparing");
                        text(status, "Playback audio is still preparing. Press Play again in a moment.");
                    }
                    204 => {
                        pending_play = None;
                        play_after_position = false;
                        resume_after_drag = false;
                        if playing {
                            if let Some(p) = &player {
                                let _ = p.Pause();
                            }
                            playing = false;
                        }
                        start = current.min((end - 0.05).max(0.0));
                        InvalidateRect(timeline, std::ptr::null(), 0);
                    }
                    205 => {
                        pending_play = None;
                        play_after_position = false;
                        resume_after_drag = false;
                        if playing {
                            if let Some(p) = &player {
                                let _ = p.Pause();
                            }
                            playing = false;
                        }
                        end = current.max(start + 0.05).min(duration);
                        InvalidateRect(timeline, std::ptr::null(), 0);
                    }
                    209 if !saving => {
                        if playing {
                            if let Some(p) = &player {
                                let _ = p.Pause();
                            }
                            playing = false;
                        }
                        start = 0.0;
                        end = duration;
                        InvalidateRect(timeline, std::ptr::null(), 0);
                    }
                    210 if saving => {
                        cancel.store(true, Ordering::SeqCst);
                        text(status, "Cancelling export...");
                    }
                    206 if !saving && !mixing && duration > 0.0 => {
                        let mut mb = [0u16; 8];
                        GetWindowTextW(targetinput, mb.as_mut_ptr(), 8);
                        let requested_mb = String::from_utf16_lossy(&mb)
                            .trim_matches(char::from(0))
                            .parse::<u32>()
                            .unwrap_or(0);
                        if SendMessageW(preset, CB_GETCURSEL, 0, 0) == 6
                            && !(1..=500).contains(&requested_mb)
                        {
                            text(status, "Choose a size limit between 1 and 500 MB.");
                            continue;
                        }
                        saving = true;
                        EnableWindow(GetDlgItem(h, 206), 0);
                        SAVING.store(true, Ordering::SeqCst);
                        cancel.store(false, Ordering::SeqCst);
                        EnableWindow(renamebutton, 0);
                        EnableWindow(deletebutton, 0);
                        EnableWindow(cancelbutton, 1);
                        ShowWindow(cancelbutton, SW_SHOW);
                        EnableWindow(preset, 0);
                        EnableWindow(targetinput, 0);
                        selected_mode = match SendMessageW(preset, CB_GETCURSEL, 0, 0) {
                            1 => crate::export::Mode::Exact,
                            2 => crate::export::Mode::Share720,
                            3 => crate::export::Mode::Small480,
                            4 => crate::export::Mode::Target10,
                            5 => crate::export::Mode::Target20,
                            6 => crate::export::Mode::TargetCustom(requested_mb),
                            _ => crate::export::Mode::Fast,
                        };
                        let mode = selected_mode;
                        let mix = if separated || mix_changed {
                            Some(audio_mix)
                        } else {
                            None
                        };
                        let cancellation = cancel.clone();
                        let progress = progress_tx.clone();
                        text(status, "Saving a new trimmed copy...");
                        let tx = tx.clone();
                        let e = engine.clone();
                        let i = input.clone();
                        std::thread::spawn(move || {
                            note(&format!("Trim requested start={start:.3}s end={end:.3}s"));
                            let result = crate::export::export(
                                &e,
                                &i,
                                start,
                                end,
                                mode,
                                mix,
                                cancellation,
                                progress,
                            );
                            note(if result.is_ok() {
                                "Trim export succeeded"
                            } else {
                                "Trim export failed"
                            });
                            let result = result.map(|path| {
                                let warning = crate::catalog::inherit(&i, &path).err();
                                if let Some(e) = &warning {
                                    note(&format!("Export saved but tag copy failed: {e}"));
                                }
                                saved_queue().lock().unwrap().push(path.clone());
                                (path, warning)
                            });
                            SAVING.store(false, Ordering::SeqCst);
                            tx.send(result).ok();
                        });
                    }
                    _ => {}
                }
            }
            if msg.message == WM_KEYDOWN
                && msg.wParam == 0x20
                && !crate::media::space_starts_play(
                    msg.hwnd == timeline,
                    msg.hwnd == h,
                    GetFocus() == playbutton,
                )
                && space_notes < 20
            {
                space_notes += 1;
                let mut cls = [0u16; 32];
                let n = GetClassNameW(msg.hwnd, cls.as_mut_ptr(), 32);
                note(&format!(
                    "Space not routed to Play: key target class={} playbutton_enabled={} mixing={mixing}",
                    String::from_utf16_lossy(&cls[..n.max(0) as usize]),
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(playbutton) != 0
                ));
            }
            // Right after the editor opens no child has focus and the key goes to the editor
            // window itself; that must start playback too instead of being dropped.
            if msg.message == WM_KEYDOWN
                && msg.wParam == 0x20
                && crate::media::space_starts_play(
                    msg.hwnd == timeline,
                    msg.hwnd == h,
                    GetFocus() == playbutton,
                )
            {
                PostThreadMessageW(
                    windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                    WM_APP + 51,
                    201,
                    0,
                );
                continue;
            }
            if msg.message == WM_APP + 54 && !saving && duration > 0.0 {
                pending_play = None;
                play_after_position = false;
                resume_after_drag = false;
                if playing {
                    if let Some(p) = &player {
                        let _ = p.Pause();
                    }
                    playing = false;
                }
                let at = msg.wParam.min(1000) as f64 / 1000.0 * duration;
                if msg.lParam == 1 {
                    start = at.min((end - 0.05).max(0.0));
                    // The playhead follows the IN edge and the preview shows the new first frame.
                    current = start;
                    SendMessageW(slider, TBM_SETPOS, 1, (start / duration * 1000.0) as isize);
                    pending_seek = Some(current);
                    user_seek = std::time::Instant::now();
                } else {
                    end = at.max(start + 0.05).min(duration);
                }
                InvalidateRect(timeline, std::ptr::null(), 0);
                continue;
            }
            if msg.message == WM_KEYDOWN
                && GetFocus() == timeline
                && [0xDB, 0xDD].contains(&msg.wParam)
            {
                PostThreadMessageW(
                    windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                    WM_APP + 51,
                    if msg.wParam == 0xDB { 204 } else { 205 },
                    0,
                );
                continue;
            }
            if msg.message == WM_APP + 53 && duration > 0.0 {
                if msg.lParam == 1 && !dragging {
                    dragging = true;
                    resume_after_drag = playing;
                    if let Some(p) = &player {
                        let _ = p.Pause();
                    }
                }
                if msg.lParam == 2 {
                    dragging = false;
                    last_seek = std::time::Instant::now() - Duration::from_secs(2);
                }
                current = msg.wParam.min(1000) as f64 / 1000.0 * duration;
                SendMessageW(slider, TBM_SETPOS, 1, msg.wParam as isize);
                pending_play = None;
                play_after_position = false;
                pending_seek = Some(current);
                user_seek = std::time::Instant::now();
            }
            if msg.message == WM_APP + 52 && msg.lParam == slider as isize && duration > 0.0 {
                current = SendMessageW(slider, 0x400, 0, 0) as f64 / 1000.0 * duration;
                let tracking = msg.wParam & 0xffff == TB_THUMBTRACK as usize;
                if tracking && !dragging {
                    dragging = true;
                    resume_after_drag = playing;
                    if let Some(p) = &player {
                        let _ = p.Pause();
                    }
                }
                pending_play = None;
                play_after_position = false;
                pending_seek = Some(current);
                user_seek = std::time::Instant::now();
                if !tracking {
                    dragging = false;
                    last_seek = std::time::Instant::now() - Duration::from_secs(2);
                }
                text(clock, &format!("{current:.3}s / {duration:.3}s"));
                SetPropW(
                    timeline,
                    wide("progress").as_ptr(),
                    ((current / duration.max(0.1) * 936.0) as usize + 1) as _,
                );
                InvalidateRect(timeline, std::ptr::null(), 0);
            }
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if !binding
            && pending_seek.is_some()
            && crate::media::seek_due(
                last_seek.elapsed(),
                dragging,
                seek_inflight.map(|t| t.elapsed()),
            )
        {
            let target = pending_seek.take().unwrap();
            let target = if trim_only && (playing || resume_after_drag) {
                target.clamp(start, (end - 0.001).max(start))
            } else {
                target
            };
            current = target;
            note(&format!(
                "Applied coalesced seek={target:.3}s dragging={dragging}"
            ));
            if let Some(p) = &player {
                let clear_bounds = bounds_active
                    && !playing
                    && !resume_after_drag
                    && crate::media::paused_seek_needs_bounds_clear(bound_key, target);
                if bounds_active && !playing && !resume_after_drag {
                    note(&format!("Paused seek target={target:.3} loaded_bounds_key={bound_key:?} clear_bounds={clear_bounds}"));
                }
                if clear_bounds {
                    if playback_bounds(p, false, 0.0, duration).is_ok() {
                        binding = true;
                        bounds_active = false;
                        bound_key = Some((false, 0, 0));
                        pending_position = Some(target);
                    }
                } else {
                    let result = seek_result(p, target);
                    seek_inflight = if result.is_ok() { Some(std::time::Instant::now()) } else { None };
                    if !dragging && resume_after_drag {
                        play_after_position = result.is_ok();
                        resume_after_drag = false;
                        if result.is_err() {
                            text(
                                status,
                                "Playback seek failed. Open externally or check Diagnostics.",
                            );
                        }
                    }
                }
            }
            last_seek = std::time::Instant::now();
        }
        if let Ok(r) = analysis_rx.try_recv() {
            match r {
                Ok(data) => {
                    note(&format!(
                        "Timeline ready frames={} peaks={}",
                        data.frames.len(),
                        data.peaks.len()
                    ));
                    let previous = GetWindowLongPtrW(timeline, GWLP_USERDATA)
                        as *mut crate::analysis::Timeline;
                    if !previous.is_null() {
                        drop(Box::from_raw(previous));
                    }
                    if data.frames_unavailable {
                        SetPropW(timeline, wide("timeline-unavailable").as_ptr(), 1usize as _);
                    }
                    let cached = RemovePropW(timeline, wide("timeline-base").as_ptr());
                    if !cached.is_null() {
                        DeleteObject(cached);
                    }
                    SetWindowLongPtrW(
                        timeline,
                        GWLP_USERDATA,
                        Box::into_raw(Box::new(data)) as isize,
                    );
                    InvalidateRect(timeline, std::ptr::null(), 1);
                }
                Err(e) => {
                    note(&format!("Timeline analysis failed: {e}"));
                    SetPropW(timeline, wide("timeline-unavailable").as_ptr(), 1usize as _);
                    InvalidateRect(timeline, std::ptr::null(), 1);
                    text(
                        status,
                        "Timeline analysis unavailable. Preview and fast trim still work.",
                    );
                }
            }
        }
        if refresh.elapsed() > Duration::from_millis(50) {
            if !preview_ok {
                preview_ok = player.as_ref().is_some_and(|p| p.GetMediaItem().is_ok());
                if preview_ok && !preview_reported {
                    preview_reported = true;
                    note("Preview media item became ready");
                    note(&format!(
                        "Preview ready: state={:?} binding={binding} mixing={mixing} focus_on_timeline={} playbutton_enabled={}",
                        player.as_ref().and_then(|p| p.GetState().ok()),
                        GetFocus() == timeline,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(playbutton) != 0
                    ));
                    SetPropW(video, wide("preview-live").as_ptr(), 1usize as _);
                    // One-time: the placeholder text must not stay once the player owns the window.
                    SetPropW(video, wide("preview-clear").as_ptr(), 1usize as _);
                    InvalidateRect(video, std::ptr::null(), 0);
                    text(
                        status,
                        "Clip loaded. Press Play to preview. Fast trim snaps to keyframes.",
                    );
                }
            }
            refresh = std::time::Instant::now();
            if let Some(p) = &player {
                if !binding && !dragging && pending_seek.is_none() {
                    if let Ok(state) = p.GetState() {
                        let actual = state == MFP_MEDIAPLAYER_STATE_PLAYING;
                        if actual != playing {
                            playing = actual;
                            SetPropW(playbutton, wide("playing").as_ptr(), playing as usize as _);
                            InvalidateRect(playbutton, std::ptr::null(), 0);
                        }
                    }
                }
                if duration <= 0.0 {
                    duration = seconds(p, true).unwrap_or(duration);
                    end = duration;
                    EnableWindow(
                        GetDlgItem(h, 206),
                        if duration > 0.0 && !saving { 1 } else { 0 },
                    );
                }
                if preview_ok
                    && !binding
                    && !play_after_position
                    && !dragging
                    && pending_seek.is_none()
                    && user_seek.elapsed() > Duration::from_millis(500)
                {
                    current = seconds(p, false).unwrap_or(current);
                }
            }
            if preview_ok
                && !dragging
                && pending_seek.is_none()
                && user_seek.elapsed() > Duration::from_millis(500)
                && GetCapture() != slider
                && duration > 0.0
            {
                SendMessageW(
                    slider,
                    TBM_SETPOS,
                    1,
                    (current / duration * 1000.0) as isize,
                );
            }
            let progress = (current / duration.max(0.1) * 936.0) as usize + 1;
            if GetPropW(timeline, wide("progress").as_ptr()) as usize != progress {
                SetPropW(timeline, wide("progress").as_ptr(), progress as _);
                InvalidateRect(timeline, std::ptr::null(), 0);
            }
            if trim_only && playing && (current >= end - 0.025 || current < start - 0.025) {
                if let Some(p) = &player {
                    let _ = p.Pause();
                    position(p, if current < start { start } else { end });
                }
                current = current.clamp(start, end);
                playing = false;
                SetPropW(playbutton, wide("playing").as_ptr(), std::ptr::null_mut());
                InvalidateRect(playbutton, std::ptr::null(), 0);
            }
            if playing && duration > 0.0 && current >= duration - 0.04 {
                playing = false;
                SetPropW(playbutton, wide("playing").as_ptr(), std::ptr::null_mut());
                InvalidateRect(playbutton, std::ptr::null(), 0);
            }
            text(clock, &format!("{current:.3}s / {duration:.3}s"));
            text(
                range,
                &format!(
                    "Start {start:.3}s     End {end:.3}s     Length {:.3}s",
                    end - start
                ),
            );
            let trim_start = (start / duration.max(0.1) * 935.0) as usize + 1;
            let trim_end = (end / duration.max(0.1) * 935.0) as usize + 1;
            if GetPropW(timeline, wide("trim-start").as_ptr()) as usize != trim_start
                || GetPropW(timeline, wide("trim-end").as_ptr()) as usize != trim_end
            {
                SetPropW(timeline, wide("trim-start").as_ptr(), trim_start as _);
                SetPropW(timeline, wide("trim-end").as_ptr(), trim_end as _);
                InvalidateRect(timeline, std::ptr::null(), 0);
            }
            if duration > 0.0 {
                SendMessageW(
                    slider,
                    TBM_SETSELSTART,
                    0,
                    (start / duration * 1000.0) as isize,
                );
                SendMessageW(slider, TBM_SETSELEND, 1, (end / duration * 1000.0) as isize);
            }
        }
        while let Ok(percent) = progress_rx.try_recv() {
            SetPropW(
                h,
                wide("export-progress").as_ptr(),
                (percent as usize + 1) as _,
            );
            InvalidateRect(
                h,
                &RECT {
                    left: 16,
                    top: 802,
                    right: 1032,
                    bottom: 806,
                },
                0,
            );
            text(
                status,
                &format!("Exporting new copy... {percent}%  /  original stays safe"),
            );
        }
        if let Ok(result) = rx.try_recv() {
            saving = false;
            RemovePropW(h, wide("export-progress").as_ptr());
            InvalidateRect(
                h,
                &RECT {
                    left: 16,
                    top: 802,
                    right: 1032,
                    bottom: 806,
                },
                0,
            );
            EnableWindow(renamebutton, 1);
            EnableWindow(deletebutton, 1);
            EnableWindow(cancelbutton, 0);
            ShowWindow(cancelbutton, SW_HIDE);
            EnableWindow(preset, 1);
            EnableWindow(targetinput, 1);
            EnableWindow(GetDlgItem(h, 206), if duration > 0.0 { 1 } else { 0 });
            match result {
                Ok((p, metadata_warning)) => {
                    let size =
                        std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) as f64 / 1_000_000.0;
                    let note = match crate::export::target_bytes(selected_mode) {
                        Some(cap) => format!(
                            "Output size {size:.2} MB. Size limit {:.0} MB.",
                            cap as f64 / 1_000_000.0
                        ),
                        None => {
                            if selected_mode == crate::export::Mode::Fast {
                                "Fast keyframe cut.".into()
                            } else {
                                "Re-encoded precise range.".into()
                            }
                        }
                    };
                    let metadata = metadata_warning
                        .map(|e| format!(" Video saved, but tags could not copy: {e}"))
                        .unwrap_or_default();
                    text(
                        status,
                        &format!(
                            "Saved {}. {note}{metadata}",
                            p.file_name().unwrap().to_string_lossy()
                        ),
                    );
                }
                Err(e) => text(
                    status,
                    &if e.starts_with("Export cancelled") {
                        e
                    } else {
                        format!("Export failed: {e}")
                    },
                ),
            }
        }
        if playing && last_beat.elapsed() >= Duration::from_secs(2) {
            last_beat = std::time::Instant::now();
            if let Some(p) = &player {
                note(&format!("BEAT clock={current:.3} player_pos={:?} {}", seconds(p, false), video_state(p, video)));
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

unsafe extern "system" fn timeline_proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_GETDLGCODE => DLGC_WANTARROWS as isize | DLGC_WANTCHARS as isize,
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP
            if m == WM_LBUTTONDOWN || GetCapture() == h =>
        {
            if m == WM_LBUTTONDOWN {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(h);
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetCapture(h);
            }
            let mut bounds = RECT::default();
            GetClientRect(h, &mut bounds);
            let x = ((l as i16 as i32) * 936 / bounds.right.max(1)).clamp(0, 935);
            if m == WM_LBUTTONDOWN {
                let gw = grip_width(h);
                let a = GetPropW(h, wide("trim-start").as_ptr()) as i32 - 1;
                let b = GetPropW(h, wide("trim-end").as_ptr()) as i32 - 1;
                SetPropW(
                    h,
                    wide("range-drag").as_ptr(),
                    // The whole drawn 16 px grip is grabbable (plus 4 px outside); on a narrow
                    // selection where both zones overlap, the nearer edge wins.
                    if (-4..=gw).contains(&(x - a)) && (!(-4..=gw).contains(&(b - x)) || (x - a).abs() <= (b - x).abs()) {
                        1usize as _
                    } else if (-4..=gw).contains(&(b - x)) {
                        2usize as _
                    } else {
                        std::ptr::null_mut()
                    },
                );
            }
            let handle = GetPropW(h, wide("range-drag").as_ptr()) as usize;
            if handle > 0 {
                PostThreadMessageW(
                    windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                    WM_APP + 54,
                    x as usize * 1000 / 936,
                    handle as isize,
                );
                if m == WM_LBUTTONUP {
                    RemovePropW(h, wide("range-drag").as_ptr());
                    let _ = windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                }
                return 0;
            }
            PostThreadMessageW(
                windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                WM_APP + 53,
                (x as usize) * 1000 / 936,
                if m == WM_LBUTTONUP { 2 } else { 1 },
            );
            if m == WM_LBUTTONUP {
                let _ = windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            0
        }
        WM_DESTROY => {
            let cached = RemovePropW(h, wide("timeline-base").as_ptr());
            if !cached.is_null() {
                DeleteObject(cached);
            }
            let ptr = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut crate::analysis::Timeline;
            if !ptr.is_null() {
                RemovePropW(h, wide("native-player").as_ptr());
                drop(Box::from_raw(ptr));
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let screen = BeginPaint(h, &mut ps);
            let dc = CreateCompatibleDC(screen);
            let cached = GetPropW(h, wide("timeline-base").as_ptr()) as HBITMAP;
            let bitmap = if cached.is_null() {
                CreateCompatibleBitmap(screen, 936, 196)
            } else {
                cached
            };
            let oldbitmap = SelectObject(dc, bitmap);
            if cached.is_null() {
                let oldfont = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
                let r = RECT {
                    left: 0,
                    top: 0,
                    right: 936,
                    bottom: 196,
                };
                let b = CreateSolidBrush(crate::ui::bg());
                FillRect(dc, &r, b);
                DeleteObject(b);
                let ptr = GetWindowLongPtrW(h, GWLP_USERDATA) as *const crate::analysis::Timeline;
                if !ptr.is_null() {
                    let data = &*ptr;
                    for (i, frame) in data.frames.iter().enumerate() {
                        if frame.len() < 112 * 64 * 4 {
                            continue;
                        }
                        let mut info: BITMAPINFO = std::mem::zeroed();
                        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                        info.bmiHeader.biWidth = 112;
                        info.bmiHeader.biHeight = -64;
                        info.bmiHeader.biPlanes = 1;
                        info.bmiHeader.biBitCount = 32;
                        StretchDIBits(
                            dc,
                            (i * 117) as i32,
                            0,
                            116,
                            64,
                            0,
                            0,
                            112,
                            64,
                            frame.as_ptr() as _,
                            &info,
                            DIB_RGB_COLORS,
                            SRCCOPY,
                        );
                    }
                    if data.sources.len() == 3 {
                        for (lane, (name, color)) in
                            [("System", 0x78df9a), ("Mic", 0xe8bf74), ("Comms", 0xe79db5)]
                                .iter()
                                .enumerate()
                        {
                            let top = 66 + lane as i32 * 36;
                            let mid = top + 18;
                            let band = RECT {
                                left: 0,
                                top,
                                right: 936,
                                bottom: top + 35,
                            };
                            let br = CreateSolidBrush(if lane % 2 == 0 {
                                crate::ui::panel()
                            } else {
                                crate::ui::bg()
                            });
                            FillRect(dc, &band, br);
                            DeleteObject(br);
                            SetBkMode(dc, TRANSPARENT as _);
                            SetTextColor(dc, *color);
                            let pen = CreatePen(PS_SOLID, 1, *color);
                            let old = SelectObject(dc, pen);
                            for (i, p) in data.sources[lane].iter().enumerate() {
                                let x = i as i32 * 2 + 1;
                                let v = ((p * 4.0).sqrt().min(1.0) * 11.0) as i32;
                                MoveToEx(dc, x, mid - v, std::ptr::null_mut());
                                LineTo(dc, x, mid + v + 1);
                            }
                            SelectObject(dc, old);
                            DeleteObject(pen);
                        }
                    } else {
                        let pen = CreatePen(PS_SOLID, 1, crate::ui::accent());
                        let old = SelectObject(dc, pen);
                        for (i, p) in data.peaks.iter().enumerate() {
                            let x = (i * 2 + 1) as i32;
                            let v = (p.sqrt() * 22.0) as i32;
                            MoveToEx(dc, x, 126 - v, std::ptr::null_mut());
                            LineTo(dc, x, 127 + v);
                        }
                        SelectObject(dc, old);
                        DeleteObject(pen);
                    }
                    SetTextColor(dc, crate::ui::muted());
                    SetBkMode(dc, TRANSPARENT as _);
                } else {
                    SetTextColor(dc, crate::ui::muted());
                    SetBkMode(dc, TRANSPARENT as _);
                    let mut rr = r;
                    DrawTextW(
                        dc,
                        wide(if GetPropW(h, wide("timeline-unavailable").as_ptr()).is_null() {
                            "Building filmstrip + waveform..."
                        } else {
                            "Filmstrip unavailable for this clip"
                        })
                        .as_ptr(),
                        -1,
                        &mut rr,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                }
                SelectObject(dc, oldfont);
                SetPropW(h, wide("timeline-base").as_ptr(), bitmap as _);
            }
            let frame = CreateCompatibleDC(screen);
            let framebitmap = CreateCompatibleBitmap(screen, 936, 196);
            let oldframe = SelectObject(frame, framebitmap);
            BitBlt(frame, 0, 0, 936, 196, dc, 0, 0, SRCCOPY);
            let basedc = dc;
            let dc = frame;
            let a = GetPropW(h, wide("trim-start").as_ptr()) as usize;
            let b = GetPropW(h, wide("trim-end").as_ptr()) as usize;
            if b > a && a > 0 {
                let t = crate::ui::style_theme();
                let left = (a - 1).min(935) as i32;
                let right = (b - 1).min(935) as i32;
                let shade = CreateCompatibleDC(dc);
                let bmp = CreateCompatibleBitmap(dc, 1, 1);
                let prev = SelectObject(shade, bmp);
                PatBlt(shade, 0, 0, 1, 1, BLACKNESS);
                for (x, width) in [
                    (0, (a - 1).min(935) as i32),
                    ((b - 1).min(935) as i32, 936 - (b - 1).min(935) as i32),
                ] {
                    if width > 0 {
                        AlphaBlend(
                            dc,
                            x,
                            0,
                            width,
                            178,
                            shade,
                            0,
                            0,
                            1,
                            1,
                            BLENDFUNCTION {
                                BlendOp: AC_SRC_OVER as u8,
                                BlendFlags: 0,
                                SourceConstantAlpha: 135,
                                AlphaFormat: 0,
                            },
                        );
                    }
                }
                SelectObject(shade, prev);
                DeleteObject(bmp);
                DeleteDC(shade);
                for rr in [
                    RECT {
                        left,
                        top: 0,
                        right,
                        bottom: 3,
                    },
                    RECT {
                        left,
                        top: 175,
                        right,
                        bottom: 178,
                    },
                ] {
                    FillRect(dc, &rr, crate::ui::style::brush(t.accent));
                }
                // Thick rounded grips on both ends (16 screen px wide at any control width, since the
                // strip is stretched), with grip lines so they read as draggable.
                let gw = grip_width(h);
                for gx in [left, (right - gw).max(left)] {
                    let grip = RECT { left: gx, top: 0, right: gx + gw, bottom: 178 };
                    crate::ui::style::rect(dc, grip, t.accent, t.accent, gw / 2);
                    for dx in [gw / 2 - 3, gw / 2, gw / 2 + 3] {
                        let line = RECT { left: gx + dx, top: 62, right: gx + dx + 1, bottom: 116 };
                        FillRect(dc, &line, crate::ui::style::brush(t.bg));
                    }
                }
            }
            let progress = GetPropW(h, wide("progress").as_ptr()) as usize;
            if progress > 0 {
                let x = (progress - 1).min(935) as i32;
                let head = x.clamp(7, 928);
                let p = CreatePen(PS_SOLID, 2, crate::ui::text_color());
                let old = SelectObject(dc, p);
                MoveToEx(dc, x, 0, std::ptr::null_mut());
                LineTo(dc, x, 178);
                let brush = CreateSolidBrush(crate::ui::text_color());
                let previous = SelectObject(dc, brush);
                let points = [
                    POINT { x: head - 6, y: 8 },
                    POINT { x: head + 6, y: 8 },
                    POINT { x: head, y: 20 },
                ];
                Polygon(dc, points.as_ptr(), 3);
                SelectObject(dc, previous);
                DeleteObject(brush);
                SelectObject(dc, old);
                DeleteObject(p);
            }
            let mut bounds = RECT::default();
            GetClientRect(h, &mut bounds);
            SetStretchBltMode(screen, COLORONCOLOR as _);
            StretchBlt(screen, 0, 0, bounds.right, 178, dc, 0, 0, 936, 178, SRCCOPY);
            SelectObject(frame, oldframe);
            DeleteObject(framebitmap);
            DeleteDC(frame);
            SelectObject(basedc, oldbitmap);
            DeleteDC(basedc);
            EndPaint(h, &ps);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

unsafe extern "system" fn video_proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let dc = BeginPaint(h, &mut ps);
            let ptr = GetPropW(h, wide("native-player").as_ptr()) as *mut IMFPMediaPlayer;
            if !ptr.is_null() && !GetPropW(h, wide("preview-live").as_ptr()).is_null() {
                if !RemovePropW(h, wide("preview-clear").as_ptr()).is_null() {
                    let mut r = std::mem::zeroed();
                    GetClientRect(h, &mut r);
                    let b = CreateSolidBrush(0x0b0b0b);
                    FillRect(dc, &r, b);
                    DeleteObject(b);
                }
                let hr = match (*ptr).UpdateVideo() {
                    Ok(()) => 0,
                    Err(e) => e.code().0,
                };
                VIDEO_PAINTS.fetch_add(1, Ordering::Relaxed);
                VIDEO_UPDATE_HR.store(hr, Ordering::Relaxed);
            } else {
                let mut r = std::mem::zeroed();
                GetClientRect(h, &mut r);
                let b = CreateSolidBrush(0x0b0b0b);
                FillRect(dc, &r, b);
                DeleteObject(b);
                SetBkMode(dc, TRANSPARENT as _);
                SetTextColor(dc, crate::ui::muted());
                DrawTextW(
                    dc,
                    wide("Preview unavailable in this environment").as_ptr(),
                    -1,
                    &mut r,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            let t = crate::ui::style_theme();
            crate::ui::style::rect(
                dc,
                RECT {
                    left: 14,
                    top: 14,
                    right: 163,
                    bottom: 42,
                },
                t.bg,
                t.border,
                20,
            );
            crate::ui::style::label(
                dc,
                "Original / untouched",
                RECT {
                    left: 24,
                    top: 14,
                    right: 160,
                    bottom: 42,
                },
                crate::ui::style::font(&crate::display::font_name(), 13, 600),
                t.muted,
                DT_SINGLELINE | DT_VCENTER,
            );
            EndPaint(h, &ps);
            0
        }
        WM_DESTROY => {
            let ptr = GetPropW(h, wide("native-player").as_ptr()) as *mut IMFPMediaPlayer;
            if !ptr.is_null() {
                RemovePropW(h, wide("native-player").as_ptr());
                drop(Box::from_raw(ptr));
            }
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

unsafe extern "system" fn file_dialog_proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_CLOSE => {
            SetPropW(h, wide("answer").as_ptr(), 2usize as _);
            0
        }
        WM_HSCROLL => {
            let id = GetDlgCtrlID(l as HWND);
            if (430..=432).contains(&id) {
                SetWindowTextW(
                    GetDlgItem(h, id + 10),
                    wide(&format!("{}%", SendMessageW(l as HWND, 0x400, 0, 0))).as_ptr(),
                );
            }
            0
        }
        WM_COMMAND => {
            let id = w & 0xffff;
            if (450..=452).contains(&id) {
                let index = id - 450;
                let slider = GetDlgItem(h, 430 + index as i32);
                SendMessageW(slider, TBM_SETPOS, 1, 100);
                SetWindowTextW(GetDlgItem(h, 440 + index as i32), wide("100%").as_ptr());
                InvalidateRect(slider, std::ptr::null(), 0);
                return 0;
            }
            if (460..460 + crate::welcome_logic::COMMON_APPS.len()).contains(&id) {
                // Common app chip: add or remove its names; other typed entries stay.
                let field = GetDlgItem(h, 403);
                let mut b = [0u16; 2048];
                let n = GetWindowTextW(field, b.as_mut_ptr(), 2048).max(0) as usize;
                let cur = String::from_utf16_lossy(&b[..n]);
                let next = crate::welcome_logic::toggle_group(&cur, crate::welcome_logic::COMMON_APPS[id - 460].1);
                SetWindowTextW(field, wide(&next).as_ptr());
                for i in 0..crate::welcome_logic::COMMON_APPS.len() {
                    InvalidateRect(GetDlgItem(h, 460 + i as i32), std::ptr::null(), 0);
                }
                return 0;
            }
            if (410..=414).contains(&id) {
                let secs = [15, 30, 60, 120, 300][id - 410];
                SetWindowTextW(GetDlgItem(h, 403), wide(&secs.to_string()).as_ptr());
                return 0;
            }
            if (470..=483).contains(&id) {
                SetPropW(h, wide("icon-sel").as_ptr(), (id - 470 + 1) as _);
                for i in 470..=483 {
                    InvalidateRect(GetDlgItem(h, i), std::ptr::null(), 0);
                }
                return 0;
            }
            if id == 484 {
                SetPropW(h, wide("icon-sel").as_ptr(), 200usize as _);
                SetPropW(h, wide("answer").as_ptr(), 1usize as _);
                return 0;
            }
            if id == 404 || id == 405 {
                let b = GetDlgItem(h, id as i32);
                let on = !GetPropW(b, wide("chk").as_ptr()).is_null();
                SetPropW(b, wide("chk").as_ptr(), if on { std::ptr::null_mut() } else { 1usize as _ });
                InvalidateRect(b, std::ptr::null(), 0);
                return 0;
            }
            if id == 401 || id == 402 || id == 2 {
                SetPropW(
                    h,
                    wide("answer").as_ptr(),
                    (if id == 401 { 1usize } else { 2 }) as _,
                );
            }
            0
        }
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let (dc, b) = crate::ui::begin_buffer(h, &mut ps);
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            let brush = CreateSolidBrush(crate::ui::bg());
            FillRect(dc, &r, brush);
            DeleteObject(brush);
            crate::ui::end_buffer(h, &ps, dc, b);
            0
        }
        WM_ERASEBKGND => 1,
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            SetTextColor(w as _, crate::ui::text_color());
            SetBkColor(w as _, crate::ui::bg());
            static BRUSH: std::sync::OnceLock<
                std::sync::Mutex<std::collections::BTreeMap<u32, usize>>,
            > = std::sync::OnceLock::new();
            let mut b = BRUSH.get_or_init(Default::default).lock().unwrap();
            *b.entry(crate::ui::bg())
                .or_insert_with(|| CreateSolidBrush(crate::ui::bg()) as usize) as isize
        }
        WM_DRAWITEM => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            let id = d.CtlID as usize;
            if (460..460 + crate::welcome_logic::COMMON_APPS.len()).contains(&id) {
                let t = crate::ui::style_theme();
                let mut b = [0u16; 2048];
                let n = GetWindowTextW(GetDlgItem(h, 403), b.as_mut_ptr(), 2048).max(0) as usize;
                let chosen = crate::welcome_logic::group_selected(
                    &String::from_utf16_lossy(&b[..n]),
                    crate::welcome_logic::COMMON_APPS[id - 460].1,
                );
                let mut text = [0u16; 40];
                let k = GetWindowTextW(d.hwndItem, text.as_mut_ptr(), 40).max(0) as usize;
                FillRect(d.hDC, &d.rcItem, crate::ui::style::brush(t.bg));
                crate::ui::style::button(
                    d.hDC,
                    d.rcItem,
                    &String::from_utf16_lossy(&text[..k]),
                    if chosen { crate::ui::style::Button::Primary } else { crate::ui::style::Button::Default },
                    crate::ui::style::State {
                        pressed: d.itemState & ODS_SELECTED != 0,
                        focused: GetFocus() == d.hwndItem,
                        disabled: d.itemState & ODS_DISABLED != 0,
                        ..Default::default()
                    },
                    t,
                    crate::ui::style::font(&crate::display::font_name(), 14, 400),
                );
                return 1;
            }
            if (470..=483).contains(&id) {
                let t = crate::ui::style_theme();
                let i = id - 470;
                let on = GetPropW(h, wide("icon-sel").as_ptr()) as usize == i + 1;
                FillRect(d.hDC, &d.rcItem, crate::ui::style::brush(t.bg));
                crate::ui::style::rect(d.hDC, d.rcItem, if on { t.accent } else { t.raise }, if on { t.accent } else { t.border }, 14);
                crate::polished::icon(d.hDC, &format!("brand{i}"), d.rcItem.left + 12, d.rcItem.top + 12, 28, if on { t.ink } else { t.text });
                if GetFocus() == d.hwndItem {
                    crate::ui::style::focus(d.hDC, d.rcItem, t);
                }
                return 1;
            }
            if id == 404 || id == 405 {
                let t = crate::ui::style_theme();
                let on = !GetPropW(d.hwndItem, wide("chk").as_ptr()).is_null();
                let mut text = [0u16; 96];
                let k = GetWindowTextW(d.hwndItem, text.as_mut_ptr(), 96).max(0) as usize;
                FillRect(d.hDC, &d.rcItem, crate::ui::style::brush(t.bg));
                let cy = (d.rcItem.top + d.rcItem.bottom) / 2;
                let boxr = RECT { left: d.rcItem.left + 2, top: cy - 10, right: d.rcItem.left + 22, bottom: cy + 10 };
                crate::ui::style::rect(d.hDC, boxr, if on { t.accent } else { t.raise }, if on { t.accent } else { t.border }, 7);
                if on {
                    crate::polished::icon(d.hDC, "check", boxr.left + 3, boxr.top + 3, 14, t.ink);
                }
                let mut tr = d.rcItem;
                tr.left += 32;
                crate::ui::style::label(
                    d.hDC,
                    &String::from_utf16_lossy(&text[..k]),
                    tr,
                    crate::ui::style::font(&crate::display::font_name(), 14, 400),
                    t.text,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
                if GetFocus() == d.hwndItem {
                    crate::ui::style::focus(d.hDC, d.rcItem, t);
                }
                return 1;
            }
            crate::ui::handle(h, m, w, l).unwrap_or(0)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
thread_local! { static OK_LABEL: std::cell::RefCell<String> = std::cell::RefCell::new(String::from("Continue")); }
/// Themed confirmation. The first line of `text` is the title, the rest is the message.
pub unsafe fn confirm_themed(parent: HWND, font: HFONT, text: &str, ok: &str) -> bool {
    OK_LABEL.with(|l| *l.borrow_mut() = ok.to_string());
    file_dialog_inner(parent, font, false, text, 8).is_some()
}
pub unsafe fn confirm_restart(parent: HWND, font: HFONT) -> bool {
    confirm_themed(
        parent,
        font,
        "Restart capture?\nChanging this setting restarts capture and clears the unsaved replay buffer.\nSaved clips are not affected.",
        "Continue",
    )
}
pub unsafe fn comms_dialog(parent: HWND, font: HFONT) -> Option<String> {
    file_dialog_inner(parent, font, true, &crate::comms::configured(), 7).map(|(s, _)| s)
}
pub unsafe fn library_filter_dialog(
    parent: HWND,
    font: HFONT,
    query: &str,
    oldest: bool,
) -> Option<(String, bool)> {
    file_dialog_inner(parent, font, true, query, if oldest { 5 } else { 4 })
}
pub unsafe fn tags_dialog(parent: HWND, font: HFONT, tags: &str) -> Option<String> {
    let mut draft = tags.to_string();
    loop {
        let (value, _) = file_dialog_inner(parent, font, true, &draft, 6)?;
        match crate::catalog::normalize(&value) {
            Ok(v) => return Some(v),
            Err(e) => {
                draft = value;
                MessageBoxW(
                    parent,
                    wide(&e).as_ptr(),
                    wide("Tags unchanged").as_ptr(),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
        }
    }
}
pub unsafe fn buffer_dialog(parent: HWND, font: HFONT, seconds: u64) -> Option<String> {
    file_dialog_inner(parent, font, true, &seconds.to_string(), 2).map(|v| v.0)
}
pub unsafe fn app_name_dialog(parent: HWND, font: HFONT, name: &str) -> Option<String> {
    file_dialog_inner(parent, font, true, name, 1).map(|v| v.0)
}
pub unsafe fn icon_dialog(parent: HWND, font: HFONT, current: usize) -> Option<String> {
    file_dialog_inner(parent, font, false, &current.to_string(), 9).map(|v| v.0)
}
pub unsafe fn confirm_clip_delete(parent: HWND, font: HFONT, name: &str) -> bool {
    file_dialog_inner(parent, font, false, name, 3).is_some()
}
unsafe fn file_dialog(
    parent: HWND,
    font: HFONT,
    rename: bool,
    filename: &str,
) -> Option<(String, bool)> {
    file_dialog_inner(parent, font, rename, filename, 0)
}
unsafe fn file_dialog_inner(
    parent: HWND,
    font: HFONT,
    rename: bool,
    filename: &str,
    mode: u8,
) -> Option<(String, bool)> {
    // The icon picker uses the app font at a readable size instead of the small default.
    let font = if mode == 9 {
        crate::ui::style::font(&crate::display::font_name(), 15, 400)
    } else {
        font
    };
    let comms = mode == 7;

    let filtering = mode == 4 || mode == 5;
    let tagging = mode == 6;
    let appname = mode == 1;
    let buffer = mode == 2;
    let cls = wide("ZimClipFileDialog");
    let inst = GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(file_dialog_proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let mut r = RECT::default();
    GetWindowRect(parent, &mut r);
    let title8 = filename.lines().next().unwrap_or("Confirm").to_string();
    // Confirmation body height is measured with the real font, so longer wording or a
    // larger font grows the dialog instead of clipping.
    // Plain newlines are joined with spaces (older confirmations); \x0b is a hard line break.
    let body8 = filename.splitn(2, '\n').nth(1).unwrap_or("").replace('\n', " ").replace('\x0b', "\r\n");
    // Work area of the parent's monitor: the dialog must stay inside it on small screens.
    let mut wa = RECT { left: 0, top: 0, right: 1366, bottom: 728 };
    {
        let mon = MonitorFromWindow(parent, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) != 0 {
            wa = mi.rcWork;
        }
    }
    // Confirmation width follows the parent window so it never hangs outside a compact window.
    let pw = r.right - r.left;
    let dw: i32 = if mode == 8 { (pw - 24).clamp(420, 650).min((wa.right - wa.left - 16).max(300)) } else { 650 };
    let bw = dw - 60;
    let dx = if mode == 8 { (pw - dw) / 2 } else { 170 };
    let ph = r.bottom - r.top;
    let body_full = if mode == 8 {
        let face = crate::display::font_name();
        let f = crate::ui::style::font(&face, 15, 400);
        let screen = GetDC(std::ptr::null_mut());
        let old = SelectObject(screen, f as _);
        let mut m = RECT { left: 0, top: 0, right: bw, bottom: 0 };
        DrawTextW(screen, wide(&body8).as_ptr(), -1, &mut m, DT_CALCRECT | DT_WORDBREAK);
        SelectObject(screen, old);
        ReleaseDC(std::ptr::null_mut(), screen);
        (m.bottom - m.top + 6).max(24)
    } else {
        0
    };
    // Too tall for the work area: the body becomes a read-only scrolling box and the buttons stay visible.
    let max_body = ((wa.bottom - wa.top) - 16 - (58 + 16 + 40 + 24)).max(60);
    let body_h = body_full.min(max_body);
    let body_scrolls = body_full > body_h;
    // A tall confirmation moves up to stay inside the parent instead of hanging below it.
    let dh8 = 58 + body_h + 16 + 40 + 24;
    let dy = if mode == 8 && 220 + dh8 > ph - 8 { ((ph - dh8) / 2).max(8) } else { 220 };
    let (win_x, win_y) = if mode == 8 {
        (
            (r.left + dx).clamp(wa.left + 8, (wa.right - 8 - dw).max(wa.left + 8)),
            (r.top + dy).clamp(wa.top + 8, (wa.bottom - 8 - dh8).max(wa.top + 8)),
        )
    } else {
        (r.left + dx, r.top + dy)
    };
    let h = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_LAYERED,
        cls.as_ptr(),
        wide(if mode == 9 {
            "App icon"
        } else if mode == 8 {
            title8.as_str()
        } else if comms {
            "Comms audio source"
        } else if filtering {
            "Find clips / sort"
        } else if tagging {
            "Clip tags"
        } else if buffer {
            "Replay length"
        } else if appname {
            "App name / icon"
        } else if rename {
            "Rename replay"
        } else {
            "Delete replay?"
        })
        .as_ptr(),
        WS_POPUP | WS_CLIPCHILDREN,
        win_x,
        win_y,
        dw,
        if comms { 340 } else if mode == 9 { 310 } else if mode == 8 { 58 + body_h + 16 + 40 + 24 } else { 290 },
        parent,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    crate::ui::titlebar(h);
    crate::ui::clip_round(h, 26);
    let label = CreateWindowExW(
        0,
        wide("STATIC").as_ptr(),
        wide(&if mode == 9 {String::from("Choose an icon for the window and tray.\nIt follows your accent colour. The EXE and pinned icons stay unchanged.")} else if mode == 8 {title8.clone()} else if comms {String::from("Voice chat apps: file names separated by commas, without .exe.\nClick a chip to add or remove it, or type other names below. Saved; matches running audio processes.") } else if filtering{String::from("Find by filename or tag. Use #tag for an exact tag.\nAll search words must match. Clear the field to show every clip.")}else if tagging{String::from("Tags separated by commas. Up to eight tags, 24 characters each.\nLeave blank to remove tags. Videos are not modified.")}else if buffer{String::from("Replay length in seconds (5 to 300). Applying starts a fresh buffer.\nLonger replay uses more memory, capped at 192 MiB compressed.\nNear the cap, saves may be shorter. Saved clips stay untouched.")}else if appname{String::from("Choose your app name (up to 24 characters). Blank resets to ZimClip.\nIcon follows Accent color while running. EXE / pinned icons stay unchanged.")}else if rename {
            format!("Rename {filename}\nUse a name only. The .mp4 extension stays.")
        } else {
            format!("Delete {filename}?\n{}", if mode==3 && filename.contains("selected clips"){"They will move to Clips\\Trash. You can recover them there."}else{"It will move to Clips\\Trash. You can recover it there."})
        })
        .as_ptr(),
        WS_CHILD | WS_VISIBLE,
        24,
        20,
        if mode == 8 { bw } else { 590 },
        if mode == 8 { 32 } else { 58 },
        h,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    SendMessageW(label, WM_SETFONT, font as usize, 0);
    if mode == 8 {
        // Content-sized confirmation: a clear title and a readable body in the app font.
        let face = crate::display::font_name();
        SendMessageW(label, WM_SETFONT, crate::ui::style::font(&face, 20, 600) as usize, 0);
        let b = if body_scrolls {
            CreateWindowExW(0, wide("EDIT").as_ptr(), wide(&body8).as_ptr(), WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_TABSTOP | (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32, 24, 58, bw, body_h, h, std::ptr::null_mut(), inst, std::ptr::null())
        } else {
            CreateWindowExW(0, wide("STATIC").as_ptr(), wide(&body8).as_ptr(), WS_CHILD | WS_VISIBLE, 24, 58, bw, body_h, h, std::ptr::null_mut(), inst, std::ptr::null())
        };
        SendMessageW(b, WM_SETFONT, crate::ui::style::font(&face, 15, 400) as usize, 0);
    }
    let field = if rename {
        let c = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            wide("EDIT").as_ptr(),
            wide(if appname || buffer || filtering || tagging {
                filename
            } else {
                filename.strip_suffix(".mp4").unwrap_or(filename)
            })
            .as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            24,
            92,
            580,
            32,
            h,
            403usize as _,
            inst,
            std::ptr::null(),
        );
        SendMessageW(c, WM_SETFONT, font as usize, 1);
        SendMessageW(
            c,
            EM_SETLIMITTEXT,
            if buffer {
                3
            } else if appname {
                24
            } else {
                if tagging {
                    512
                } else if comms {
                    2048
                } else {
                    100
                }
            },
            0,
        );
        crate::ui::clip_round(c, 12);
        c
    } else {
        let c = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("Don't show this confirmation again").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            24,
            92,
            580,
            32,
            h,
            404usize as _,
            inst,
            std::ptr::null(),
        );
        SendMessageW(c, WM_SETFONT, font as usize, 1);
        c
    };
    let oldestbox = if filtering {
        let c = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("Oldest first (unchecked: newest first)").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            24,
            132,
            580,
            30,
            h,
            405usize as _,
            inst,
            std::ptr::null(),
        );
        SendMessageW(c, WM_SETFONT, font as usize, 1);
        SetPropW(c, wide("chk").as_ptr(), if mode == 5 { 1usize as _ } else { std::ptr::null_mut() });
        c
    } else {
        std::ptr::null_mut()
    };
    if mode == 9 {
        let cur: usize = filename.trim().parse().unwrap_or(0);
        SetPropW(h, wide("icon-sel").as_ptr(), (cur + 1) as _);
        for i in 0..=crate::welcome_draw::GLYPH_ICONS {
            let (col, row) = ((i % 7) as i32, (i / 7) as i32);
            child(h, "BUTTON", "", 470 + i, 24 + col * 66, 92 + row * 66, 52, 52, font);
        }
        child(h, "BUTTON", "Upload image...", 484, 500, 92, 126, 40, font);
    }
    if mode == 3 || mode == 8 || mode == 9 {
        ShowWindow(field, SW_HIDE);
    }
    if buffer {
        for (i, text) in ["15s", "30s", "1 min", "2 min", "5 min"].iter().enumerate() {
            child(
                h,
                "BUTTON",
                text,
                410 + i,
                24 + i as i32 * 116,
                132,
                108,
                32,
                font,
            );
        }
    }
    if comms {
        for (i, (name, _)) in crate::welcome_logic::COMMON_APPS.iter().enumerate() {
            let (col, row) = ((i % 4) as i32, (i / 4) as i32);
            child(h, "BUTTON", name, 460 + i, 24 + col * 150, 136 + row * 44, 140, 36, font);
        }
    }
    let by = if comms { 257 } else if mode == 9 { 236 } else if mode == 8 { 58 + body_h + 16 } else { 177 };
    let ok8 = OK_LABEL.with(|l| l.borrow().clone());
    child(h, "BUTTON", "Cancel", 402, if mode == 8 { dw - 350 } else { 300 }, by, 140, 40, font);
    child(
        h,
        "BUTTON",
        if mode == 9 {
            "Apply icon"
        } else if mode == 8 {
            ok8.as_str()
        } else if comms {
            "Apply source"
        } else if filtering {
            "Apply filter"
        } else if tagging {
            "Save tags"
        } else if buffer {
            "Apply length"
        } else if appname {
            "Apply name"
        } else if rename {
            "Rename"
        } else {
            "Move to Trash"
        },
        401,
        if mode == 8 { dw - 194 } else { 456 },
        by,
        150,
        40,
        font,
    );
    EnableWindow(parent, 0);
    SetLayeredWindowAttributes(h, 0, 0, LWA_ALPHA);
    ShowWindow(h, SW_SHOW);
    SetForegroundWindow(h);
    let (final_x, final_y) = (win_x, win_y);
    let animate = crate::ui::motion::allowed(h);
    let opened = std::time::Instant::now();
    let mut settled = false;
    if !animate {
        SetLayeredWindowAttributes(h, 0, 255, LWA_ALPHA);
        settled = true;
    }
    windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(if rename {
        field
    } else {
        GetDlgItem(h, 402)
    });
    if rename && !comms {
        SendMessageW(field, EM_SETSEL, 0, -1);
    }
    let mut msg = MSG::default();
    let mut answer = 0;
    while answer == 0 {
        if !settled {
            // Pop-in: about 150 ms fade with a 6 px rise, ease-out.
            let t = (opened.elapsed().as_millis() as f32 / 150.0).min(1.0);
            let e = 1.0 - (1.0 - t) * (1.0 - t);
            SetLayeredWindowAttributes(h, 0, (e * 255.0) as u8, LWA_ALPHA);
            SetWindowPos(h, std::ptr::null_mut(), final_x, final_y + ((1.0 - e) * 6.0) as i32, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            settled = t >= 1.0;
        }
        if PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) == 0 {
            answer = GetPropW(h, wide("answer").as_ptr()) as usize;
            std::thread::sleep(Duration::from_millis(if settled { 5 } else { 8 }));
            continue;
        }
        if msg.message == WM_QUIT {
            PostQuitMessage(msg.wParam as i32);
            break;
        }
        if msg.message == WM_KEYDOWN && msg.wParam == 0x1b {
            answer = 2;
            break;
        }
        if msg.message == WM_KEYDOWN && msg.wParam == 0x0d {
            answer = if rename || GetFocus() == GetDlgItem(h, 401) {
                1
            } else {
                2
            };
            break;
        }
        if IsDialogMessageW(h, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        answer = GetPropW(h, wide("answer").as_ptr()) as usize;
    }
    let mut text = vec![0u16; (GetWindowTextLengthW(field) + 1).max(1) as usize];
    let n = if rename {
        GetWindowTextW(field, text.as_mut_ptr(), text.len() as i32)
    } else {
        0
    };
    let checked = if filtering {
        !GetPropW(oldestbox, wide("chk").as_ptr()).is_null()
    } else {
        !rename && mode != 3 && mode != 8 && !GetPropW(field, wide("chk").as_ptr()).is_null()
    };
    let value = String::from_utf16_lossy(&text[..n as usize]);
    let icon_sel = GetPropW(h, wide("icon-sel").as_ptr()) as usize;
    let value = if mode == 9 {
        match icon_sel {
            200 => String::from("upload"),
            1 => String::from("default"),
            n if n >= 2 => format!("glyph:{}", n - 1),
            _ => String::from("default"),
        }
    } else {
        value
    };
    DestroyWindow(h);
    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RedrawWindow(
        parent,
        std::ptr::null(),
        std::ptr::null_mut(),
        RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
    );
    if answer == 1 {
        Some((value, checked))
    } else {
        None
    }
}

unsafe fn mixer_dialog(
    parent: HWND,
    font: HFONT,
    mix: crate::mixer::AudioMix,
) -> Option<crate::mixer::AudioMix> {
    let cls = wide("ZimClipMixer");
    let inst = GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(file_dialog_proc),
        hInstance: inst,
        lpszClassName: cls.as_ptr(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    let mut r = RECT::default();
    GetWindowRect(parent, &mut r);
    let h = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        cls.as_ptr(),
        wide("Audio mixer").as_ptr(),
        WS_POPUP | WS_CLIPCHILDREN,
        r.left + 160,
        r.top + 180,
        650,
        340,
        parent,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    crate::ui::titlebar(h);
    crate::ui::clip_round(h, 24);
    SetPropW(h, wide("surface-bg").as_ptr(), 1usize as _);
    child(
        h,
        "STATIC",
        "Audio mixer / preview and export",
        0,
        24,
        18,
        602,
        28,
        font,
    );
    child(
        h,
        "STATIC",
        "0 = mute, 100 = original, 200 = double. Sources stay unchanged.",
        0,
        24,
        52,
        602,
        28,
        font,
    );
    let mut sliders = Vec::new();
    for (i, (name, value)) in [
        ("System", mix.system),
        ("Mic", mix.mic),
        ("Comms", mix.comms),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 96 + i as i32 * 48;
        child(h, "STATIC", name, 0, 24, y + 6, 100, 24, font);
        let c = child(h, "msctls_trackbar32", "", 430 + i, 132, y, 310, 32, font);
        child(
            h,
            "STATIC",
            &format!("{value}%"),
            440 + i,
            456,
            y + 6,
            62,
            24,
            font,
        );
        child(h, "BUTTON", "Reset", 450 + i, 530, y, 94, 32, font);
        SendMessageW(c, TBM_SETRANGEMAX, 1, 200);
        SendMessageW(c, TBM_SETPOS, 1, value as isize);
        SetPropW(c, wide("surface-bg").as_ptr(), 1usize as _);
        crate::ui::modern_slider(c);
        sliders.push(c);
    }
    child(h, "BUTTON", "Cancel", 402, 300, 270, 140, 40, font);
    child(h, "BUTTON", "Apply mix", 401, 458, 270, 166, 40, font);
    EnableWindow(parent, 0);
    ShowWindow(h, SW_SHOW);
    SetForegroundWindow(h);
    let mut msg = MSG::default();
    let mut answer = 0;
    while answer == 0 {
        if PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_KEYDOWN && msg.wParam == 0x1b {
                break;
            }
            if msg.message == WM_QUIT {
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
        answer = GetPropW(h, wide("answer").as_ptr()) as usize;
    }
    let values: Vec<_> = sliders
        .iter()
        .map(|c| SendMessageW(*c, 0x400, 0, 0).clamp(0, 200) as u32)
        .collect();
    DestroyWindow(h);
    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RedrawWindow(
        parent,
        std::ptr::null(),
        std::ptr::null_mut(),
        RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
    );
    if answer == 1 {
        Some(crate::mixer::AudioMix {
            system: values[0],
            mic: values[1],
            comms: values[2],
        })
    } else {
        None
    }
}

unsafe fn editor_window_height() -> i32 {
    let mut r = RECT::default();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut _ as _, 0);
    // Never taller than the work area, whatever its size; the page scrolls when short.
    let avail = (r.bottom - r.top - 8).max(200);
    avail.min(904)
}
unsafe extern "system" fn editor_register_scroll(c: HWND, _: isize) -> i32 {
    let mut r = RECT::default();
    GetWindowRect(c, &mut r);
    let mut p = POINT {
        x: r.left,
        y: r.top,
    };
    ScreenToClient(GetParent(c), &mut p);
    SetPropW(c, wide("editor-origin-y").as_ptr(), (p.y + 1) as usize as _);
    windows_sys::Win32::UI::Shell::SetWindowSubclass(c, Some(editor_child_scroll), 91, 0);
    1
}
unsafe extern "system" fn editor_child_scroll(
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
        let parent = GetParent(h);
        let y = GetPropW(h, wide("editor-origin-y").as_ptr()) as i32 - 1;
        let offset = GetPropW(parent, wide("editor-scroll").as_ptr()) as i32;
        let mut r = RECT::default();
        GetClientRect(parent, &mut r);
        if y < offset {
            editor_scroll(parent, y - offset);
        } else if y + 48 > offset + r.bottom {
            editor_scroll(parent, y + 48 - offset - r.bottom);
        }
    }
    windows_sys::Win32::UI::Shell::DefSubclassProc(h, m, w, l)
}
unsafe extern "system" fn editor_move_scroll(c: HWND, offset: isize) -> i32 {
    let y = GetPropW(c, wide("editor-origin-y").as_ptr()) as i32 - 1;
    let mut r = RECT::default();
    GetWindowRect(c, &mut r);
    let mut p = POINT {
        x: r.left,
        y: r.top,
    };
    ScreenToClient(GetParent(c), &mut p);
    SetWindowPos(
        c,
        std::ptr::null_mut(),
        p.x,
        y - offset as i32,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    1
}
/// Handles of the unnamed controls the editor layouts need to move.
struct EdCtl {
    metadata: HWND,
    filename: HWND,
    video: HWND,
    clock: HWND,
    range: HWND,
    status: HWND,
    timeline: HWND,
    keycaps: HWND,
    target: HWND,
    mblabel: HWND,
}

/// Screen-pixel width of a trim grip expressed in the timeline's 936-wide drawing space.
unsafe fn grip_width(timeline: HWND) -> i32 {
    let mut b = RECT::default();
    GetClientRect(timeline, &mut b);
    (16 * 936 / b.right.max(1)).clamp(16, 40)
}

/// Layout 0 is the original single-column page. Layout 1 puts the preview, timeline and trim
/// controls on the left and the export and audio cards on the right. Both use the same
/// controls and the same handlers, so nothing is added or removed by switching.
unsafe fn editor_apply_layout(h: HWND, layout: usize, c: &EdCtl) {
    // Remember the classic client width once, so switching back restores it exactly.
    let classic_w = {
        let stored = GetPropW(h, wide("editor-classic-w").as_ptr()) as i32;
        if stored > 0 {
            stored
        } else {
            let mut r = RECT::default();
            GetClientRect(h, &mut r);
            SetPropW(h, wide("editor-classic-w").as_ptr(), r.right as usize as _);
            r.right
        }
    };
    SetPropW(h, wide("editor-scroll").as_ptr(), 0usize as _);
    SetPropW(h, wide("editor-layout").as_ptr(), layout as usize as _);
    let content = if layout == 1 { 836 } else { EDITOR_CONTENT_H };
    SetPropW(h, wide("editor-content-h").as_ptr(), content as usize as _);
    // Resize the window: client width follows the layout, height stays within the work area.
    let (mut win, mut cl) = (RECT::default(), RECT::default());
    GetWindowRect(h, &mut win);
    GetClientRect(h, &mut cl);
    let frame_w = (win.right - win.left) - cl.right;
    let frame_h = (win.bottom - win.top) - cl.bottom;
    let want_client_w = if layout == 1 { classic_w + 216 } else { classic_w };
    let mut work = RECT::default();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as _, 0);
    let want_h = (content + frame_h).min((work.bottom - work.top - 8).max(200)).min(904 + if layout == 1 { 0 } else { 0 });
    let new_w = want_client_w + frame_w;
    let x = win.left.clamp(work.left, (work.right - new_w).max(work.left));
    let y = win.top.clamp(work.top, (work.bottom - want_h).max(work.top));
    SetWindowPos(h, std::ptr::null_mut(), x, y, new_w, want_h, SWP_NOZORDER | SWP_NOACTIVATE);
    let place = |ctl: HWND, x: i32, y: i32, w: i32, hh: i32| {
        if ctl.is_null() {
            return;
        }
        SetWindowPos(ctl, std::ptr::null_mut(), x, y, w, hh, SWP_NOZORDER | SWP_NOACTIVATE);
        SetPropW(ctl, wide("editor-origin-y").as_ptr(), (y + 1) as usize as _);
    };
    let id = |n: i32| GetDlgItem(h, n);
    if layout == 1 {
        place(id(217), 24, 20, 98, 36);
        place(c.filename, 136, 18, 330, 28);
        place(c.metadata, 136, 48, 330, 22);
        place(id(218), 476, 20, 40, 36);
        place(id(213), 652, 20, 36, 36);
        place(id(214), 696, 20, 44, 36);
        place(id(207), 748, 20, 48, 36);
        place(c.video, 24, 92, 772, 334);
        place(id(201), 24, 438, 48, 36);
        place(c.clock, 84, 446, 380, 24);
        place(id(216), 614, 438, 182, 36);
        place(c.timeline, 112, 498, 684, 178);
        place(c.keycaps, 24, 700, 772, 22);
        place(id(204), 24, 732, 38, 36);
        place(id(205), 74, 732, 38, 36);
        place(c.range, 132, 740, 600, 26);
        place(id(209), 758, 732, 38, 36);
        place(id(208), 844, 70, 388, 280);
        place(c.target, 844, 118, 70, 24);
        // "MB" is painted on the card by the window itself (a control label would paint the page colour).
        place(c.mblabel, 0, 0, 0, 0);
        place(id(206), 844, 156, 388, 44);
        // The status line sits inside the Export card so save progress and the result are always visible.
        place(c.status, 844, 204, 388, 34);
        place(id(210), 844, 240, 388, 26);
        place(id(215), 844, 340, 388, 36);
    } else {
        place(id(217), 24, 20, 98, 36);
        place(c.filename, 136, 20, 510, 28);
        place(c.metadata, 136, 50, 510, 22);
        place(id(218), 666, 20, 40, 36);
        place(id(215), 716, 20, 160, 36);
        place(id(213), 888, 20, 36, 36);
        place(id(207), 936, 20, 40, 36);
        place(id(214), 988, 20, 44, 36);
        place(c.video, 24, 92, 1008, 310);
        place(id(201), 24, 414, 48, 36);
        place(c.clock, 84, 422, 400, 24);
        place(id(216), 850, 414, 182, 36);
        place(c.timeline, 112, 466, 920, 178);
        place(c.keycaps, 24, 656, 1008, 22);
        place(id(204), 24, 688, 38, 36);
        place(id(205), 74, 688, 38, 36);
        place(c.range, 132, 696, 800, 26);
        place(id(209), 994, 688, 38, 36);
        place(id(208), 24, 754, 420, 280);
        place(c.target, 454, 758, 60, 24);
        place(c.mblabel, 514, 762, 34, 24);
        place(id(206), 748, 750, 284, 48);
        place(id(210), 548, 804, 180, 30);
        place(c.status, 24, 812, 1008, 38);
    }
    // In Layout 2 the status line is on the card, so it paints the card colour.
    if layout == 1 {
        SetPropW(c.status, wide("card-label").as_ptr(), 1usize as _);
    } else {
        RemovePropW(c.status, wide("card-label").as_ptr());
    }
    // The strip keeps its rounded clip after a resize.
    crate::ui::clip_round(c.timeline, 16);
    crate::ui::clip_round(c.target, 12);
    RedrawWindow(
        h,
        std::ptr::null(),
        std::ptr::null_mut(),
        RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_FRAME,
    );
}

/// Content height of the editor page, in pixels.
const EDITOR_CONTENT_H: i32 = 862;
/// Content height of the current editor layout (the property is set when a layout is applied).
unsafe fn content_h(h: HWND) -> i32 {
    match GetPropW(h, wide("editor-content-h").as_ptr()) as i32 {
        0 => EDITOR_CONTENT_H,
        v => v,
    }
}
/// Thumb rectangle in client coordinates when the page overflows the window, else None.
unsafe fn editor_thumb(h: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let content = content_h(h);
    if r.bottom >= content || r.bottom < 80 {
        return None;
    }
    let offset = GetPropW(h, wide("editor-scroll").as_ptr()) as i32;
    let (top, size) = crate::nav::scroll_thumb(r.bottom, content, offset)?;
    Some(RECT { left: r.right - 12, top, right: r.right - 6, bottom: top + size })
}
/// Scroll so the thumb centre follows client y (click or drag in the right edge strip).
unsafe fn editor_scroll_to(h: HWND, y: i32) {
    let Some(t) = editor_thumb(h) else { return };
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let size = t.bottom - t.top;
    let span = (r.bottom - 16 - size).max(1);
    let pos = (y - 8 - size / 2).clamp(0, span);
    let target = pos * (content_h(h) - r.bottom) / span;
    let old = GetPropW(h, wide("editor-scroll").as_ptr()) as i32;
    editor_scroll(h, target - old);
}
unsafe fn editor_scroll(h: HWND, delta: i32) {
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    let old = GetPropW(h, wide("editor-scroll").as_ptr()) as i32;
    let offset = (old + delta).clamp(0, (content_h(h) - r.bottom).max(0));
    if offset == old {
        return;
    }
    SetPropW(h, wide("editor-scroll").as_ptr(), offset as usize as _);
    // No native scrollbar: the window has no WS_VSCROLL style. Content still scrolls with the
    // mouse wheel and when a control takes keyboard focus, so the bottom controls stay reachable.
    EnumChildWindows(h, Some(editor_move_scroll), offset as isize);
    RedrawWindow(
        h,
        std::ptr::null(),
        std::ptr::null_mut(),
        RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_FRAME,
    );
}

