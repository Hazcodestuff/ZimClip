use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Choices {
    pub mic: String,
    pub system: String,
}
fn state() -> &'static Mutex<Choices> {
    static S: OnceLock<Mutex<Choices>> = OnceLock::new();
    S.get_or_init(Default::default)
}
pub fn selected() -> Choices {
    state().lock().unwrap().clone()
}
pub fn load(root: &Path) {
    let text = std::fs::read_to_string(root.join("audio-devices.txt")).unwrap_or_default();
    let mut lines = text.lines();
    *state().lock().unwrap() = Choices {
        mic: lines.next().unwrap_or("").into(),
        system: lines.next().unwrap_or("").into(),
    };
}
pub fn save(root: &Path, c: Choices) -> Result<(), String> {
    if c.mic.contains(['\n', '\r']) || c.system.contains(['\n', '\r']) {
        return Err("Invalid device ID".into());
    }
    crate::save_folder::atomic_text(
        &root.join("audio-devices.txt"),
        &format!("{}\n{}\n", c.mic, c.system),
    )?;
    *state().lock().unwrap() = c;
    Ok(())
}
#[cfg(windows)]
pub fn device(mic: bool) -> Result<wasapi::Device, Box<dyn std::error::Error>> {
    let c = selected();
    let id = if mic { c.mic } else { c.system };
    let e = wasapi::DeviceEnumerator::new()?;
    if id.is_empty() {
        Ok(e.get_default_device(if mic {
            wasapi::Direction::Capture
        } else {
            wasapi::Direction::Render
        })?)
    } else {
        let d = e.get_device(&id)?;
        if d.get_state()? != wasapi::DeviceState::Active {
            return Err(
                "Selected capture device is unavailable. Choose another device or reconnect it."
                    .into(),
            );
        }
        Ok(d)
    }
}
#[cfg(windows)]
pub fn list(mic: bool) -> Result<Vec<(String, String)>, String> {
    let _ = wasapi::initialize_mta();
    let e = wasapi::DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let ds = e
        .get_device_collection(if mic {
            wasapi::Direction::Capture
        } else {
            wasapi::Direction::Render
        })
        .map_err(|e| e.to_string())?;
    let mut out = vec![(String::new(), "Follow Windows default".into())];
    for d in &ds {
        if let Ok(d) = d {
            if let (Ok(id), Ok(name)) = (d.get_id(), d.get_friendlyname()) {
                out.push((id, name))
            }
        }
    }
    Ok(out)
}
#[cfg(windows)]
pub unsafe fn dialog(
    parent: windows_sys::Win32::Foundation::HWND,
    font: windows_sys::Win32::Graphics::Gdi::HFONT,
) -> Option<Choices> {
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
    };
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
        match m {
            WM_COMMAND => {
                if [401, 402].contains(&(w & 65535)) {
                    SetPropW(h, wide("answer").as_ptr(), (w & 65535) as _);
                }
                0
            }
            WM_CLOSE => {
                SetPropW(h, wide("answer").as_ptr(), 402usize as _);
                0
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
                crate::ui::end_buffer(h, &ps, dc, b);
                0
            }
            WM_CTLCOLORSTATIC => {
                SetTextColor(w as _, crate::ui::text_color());
                SetBkColor(w as _, crate::ui::bg());
                static B: OnceLock<usize> = OnceLock::new();
                *B.get_or_init(|| CreateSolidBrush(crate::ui::bg()) as usize) as isize
            }
            _ => crate::ui::handle(h, m, w, l).unwrap_or_else(|| DefWindowProcW(h, m, w, l)),
        }
    }
    let cls = wide("ZimClipAudioDevices");
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
    let h = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        cls.as_ptr(),
        wide("Capture devices").as_ptr(),
        WS_POPUP | WS_CLIPCHILDREN,
        r.left + ((r.right - r.left) - 790) / 2,
        r.top + ((r.bottom - r.top) - 440) / 2,
        790,
        440,
        parent,
        std::ptr::null_mut(),
        inst,
        std::ptr::null(),
    );
    crate::ui::clip_round(h, 24);
    let make =
        |class: &str, t: &str, id: usize, x: i32, y: i32, width: i32, height: i32, style: u32| {
            let c = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(t).as_ptr(),
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
    make("STATIC", "Capture devices", 0, 24, 20, 730, 28, 0);
    make(
        "STATIC",
        "These choices never change Windows playback or microphone settings.",
        0,
        24,
        54,
        738,
        26,
        0,
    );
    let choices = selected();
    let mut rows = Vec::new();
    let mut controls = Vec::new();
    for (i, mic) in [true, false].into_iter().enumerate() {
        let mut list = list(mic).unwrap_or_else(|e| {
            vec![(
                String::new(),
                format!("Windows default (list unavailable: {e})"),
            )]
        });
        let id = if mic { &choices.mic } else { &choices.system };
        if !id.is_empty() && !list.iter().any(|d| &d.0 == id) {
            list.push((id.clone(), "Saved device unavailable (not replaced)".into()));
        }
        let y = 108 + i as i32 * 96;
        make(
            "STATIC",
            if mic {
                "Microphone input"
            } else {
                "System output device to capture"
            },
            0,
            24,
            y,
            730,
            26,
            0,
        );
        let c = make(
            "COMBOBOX",
            "",
            460 + i,
            24,
            y + 30,
            738,
            240,
            CBS_DROPDOWNLIST as u32
                | CBS_OWNERDRAWFIXED as u32
                | CBS_HASSTRINGS as u32
                | WS_VSCROLL
                | WS_TABSTOP,
        );
        crate::ui::dropdown(c);
        for (_, name) in &list {
            SendMessageW(c, CB_ADDSTRING, 0, wide(name).as_ptr() as isize);
        }
        let index = list.iter().position(|d| &d.0 == id).unwrap_or(0);
        SendMessageW(c, CB_SETCURSEL, index, 0);
        if !mic && crate::comms::enabled() {
            EnableWindow(c, 0);
        }
        controls.push(c);
        rows.push(list);
    }
    make(
        "STATIC",
        if crate::comms::enabled() {
            "Voice chat isolation is ON, across all output devices.\nTo capture one output device, turn Voice chat apps OFF. Your choice is kept."
        } else {
            "App isolation is OFF: System captures the chosen output device only.\nAn unavailable selected device stays silent; it is never replaced silently."
        },
        0,
        24,
        302,
        738,
        52,
        0,
    );
    make(
        "BUTTON",
        "Cancel",
        402,
        430,
        380,
        150,
        38,
        BS_OWNERDRAW as u32 | WS_TABSTOP,
    );
    make(
        "BUTTON",
        "Apply devices",
        401,
        596,
        380,
        166,
        38,
        BS_OWNERDRAW as u32 | WS_TABSTOP,
    );
    EnableWindow(parent, 0);
    ShowWindow(h, SW_SHOW);
    SetForegroundWindow(h);
    let mut msg = MSG::default();
    loop {
        if PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            if msg.message == WM_KEYDOWN && msg.wParam == 27 {
                break;
            }
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !GetPropW(h, wide("answer").as_ptr()).is_null() {
            break;
        }
    }
    let answer = GetPropW(h, wide("answer").as_ptr()) as usize;
    let value = Choices {
        mic: rows[0][SendMessageW(controls[0], CB_GETCURSEL, 0, 0).max(0) as usize]
            .0
            .clone(),
        system: rows[1][SendMessageW(controls[1], CB_GETCURSEL, 0, 0).max(0) as usize]
            .0
            .clone(),
    };
    DestroyWindow(h);
    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    if answer == 401 {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_persist_without_name_guess() {
        let d = std::env::temp_dir().join(format!("zim-devices-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let expected = Choices {
            mic: "{capture-exact-id}".into(),
            system: "{render-exact-id}".into(),
        };
        save(&d, expected.clone()).unwrap();
        *state().lock().unwrap() = Choices::default();
        load(&d);
        assert_eq!(selected(), expected);
        save(&d, Choices::default()).unwrap();
        load(&d);
        assert_eq!(selected(), Choices::default());
        std::fs::remove_dir_all(d).unwrap();
    }
}
