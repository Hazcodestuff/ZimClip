#![cfg_attr(windows, windows_subsystem = "windows")]
mod analysis;
mod home_stats;
#[cfg(windows)]
mod audio;
mod audio_devices;
mod audio_tracks;
#[cfg(windows)]
mod background;
mod capture_settings;
mod catalog;
mod clip_date;
mod clip_files;
mod favourites;
mod comms;
#[cfg(windows)]
mod diagnostics_view;
#[cfg(windows)]
mod display;
#[cfg(windows)]
mod editor;
mod export;
mod ease;
mod ctl_ids;
mod file_prefs;
#[cfg(windows)]
mod feedback;
mod identity;
mod levels;
mod library;
#[cfg(windows)]
mod library_view;
mod layout_plan;
#[cfg(windows)]
mod menu_round;
#[cfg(windows)]
mod popup_menu;
mod media;
#[cfg(windows)]
mod polished;
mod mixer;
#[cfg(windows)]
mod recording;
#[cfg(windows)]
mod replay_design;
mod ring;
mod save_folder;
#[cfg(windows)]
mod settings_view;
mod settings_bundle;
#[cfg(windows)]
mod settings_bundle_ui;
mod shortcuts;
#[cfg(windows)]
mod welcome;
mod welcome_logic;
mod aa;
mod custom_theme;
mod nav;
#[cfg(windows)]
mod custom_dialog;
mod welcome_draw;
#[cfg(windows)]
mod icon_upload;

#[cfg(windows)]
mod thumbs;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod updater;
mod update_prompt;
#[cfg(windows)]
mod app {
    use super::ring::{read_box, Ring};
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        os::windows::{io::AsRawHandle, process::CommandExt},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::{
            atomic::{AtomicBool, AtomicI32, Ordering},
            mpsc, Arc, Mutex,
        },
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{JobObjects::*, LibraryLoader::*, Threading::*},
        UI::{Controls::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    fn engine(exe: &Path) -> Command {
        let mut c = Command::new(exe);
        c.creation_flags(CREATE_NO_WINDOW);
        c
    }
    fn start(
        exe: &Path,
        index: u32,
        compatibility: bool,
        audio_status: crate::audio::Status,
        gain: Arc<AtomicI32>,
        log: Arc<Mutex<fs::File>>,
        audio_on: bool,
        width: u32,
        fps: u32,
        mic_on: bool,
        mic_gain: Arc<AtomicI32>,
        mic_status: crate::audio::Status,
    ) -> std::io::Result<Child> {
        let mut graph = if compatibility {
            format!("ddagrab=output_idx={index}:framerate={fps}:draw_mouse=1,hwdownload,format=bgra,scale=w='min({width},iw)':h=-2:flags=fast_bilinear:threads=2,format=nv12[desktop]")
        } else {
            format!("ddagrab=output_idx={index}:framerate={fps}:draw_mouse=1:shader_readable=1,vpp_amf=w='min({width},iw)':h=-2:format=nv12[desktop]")
        };
        if audio_on || mic_on || crate::comms::enabled() {
            graph.push(';');
            graph.push_str(crate::audio_tracks::capture_graph());
        }
        writeln!(
            log.lock().unwrap(),
            "RUN audio_on={audio_on} mic_on={mic_on} max_width={width} fps={fps} compatibility={compatibility}"
        )
        .ok();
        let mut command = engine(exe);
        command.args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-nostats",
            "-stats_period",
            "10",
            "-progress",
            "pipe:2",
        ]);
        if audio_on || mic_on || crate::comms::enabled() {
            command.args([
                "-probesize",
                "32",
                "-analyzeduration",
                "0",
                "-f",
                "s16le",
                "-ar",
                "48000",
                "-ac",
                "8",
                "-i",
                "pipe:0",
            ]);
        }
        command.args([
            "-filter_complex_threads",
            "2",
            "-filter_complex",
            &graph,
            "-map",
            "[desktop]",
        ]);
        if audio_on || mic_on || crate::comms::enabled() {
            command.args([
                "-map",
                "[mix]",
                "-map",
                "[system]",
                "-map",
                "[mic]",
                "-map",
                "[comms]",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-metadata:s:a:0",
                "handler_name=ZimClip Mix",
                "-metadata:s:a:1",
                "handler_name=ZimClip System",
                "-metadata:s:a:2",
                "handler_name=ZimClip Mic",
                "-metadata:s:a:3",
                "handler_name=ZimClip Comms",
                "-disposition:a:0",
                "default",
                "-disposition:a:1",
                "0",
                "-disposition:a:2",
                "0",
                "-disposition:a:3",
                "0",
            ]);
        } else {
            command.arg("-an");
        }
        command.args([
            "-c:v",
            "h264_amf",
            "-usage",
            "lowlatency",
            "-quality",
            "speed",
            "-rc",
            "cbr",
            "-b:v",
            "4000k",
            "-maxrate",
            "4000k",
            "-bufsize",
            "4000k",
            "-g",
            &fps.to_string(),
            "-bf",
            "0",
            "-fps_mode",
            "cfr",
            "-r",
            &fps.to_string(),
            "-flags",
            "+global_header",
            "-movflags",
            "+frag_keyframe+empty_moov+default_base_moof",
            "-flush_packets",
            "1",
            "-f",
            "mp4",
            "pipe:1",
        ]);
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if audio_on || mic_on || crate::comms::enabled() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .spawn()?;
        if audio_on || mic_on || crate::comms::enabled() {
            crate::audio::spawn(
                child.stdin.take().unwrap(),
                audio_status,
                gain,
                log,
                mic_on,
                mic_gain,
                mic_status,
                audio_on,
            );
        } else {
            *audio_status.lock().unwrap() = String::from("System audio OFF");
            *mic_status.lock().unwrap() = String::from("Mic OFF");
        }
        Ok(child)
    }
    fn stamp() -> String {
        let s = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("clip-{s}")
    }
    /// Local time as (year, month, day, hour, minute, second).
    fn local_time() -> (u16, u16, u16, u16, u16, u16) {
        let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
        unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t) };
        (t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
    }
    /// Base name for a new save, plus a log note when a saved template could not be used.
    fn named_base(dir: &Path) -> (String, Option<String>) {
        let style = crate::file_prefs::name_style(dir);
        if style == 0 {
            return (stamp(), None);
        }
        crate::file_prefs::save_base(dir, &crate::identity::name(), local_time(), 0)
    }
    fn name_example(dir: &Path) -> String {
        let style = crate::file_prefs::name_style(dir);
        let b = if style == 0 {
            "clip-1764222000000".to_string()
        } else {
            crate::file_prefs::save_base(dir, &crate::identity::name(), local_time(), 0).0
        };
        format!("{b}.mp4")
    }
    unsafe fn template_preview_of(edit: HWND) -> (String, String) {
        let mut b = [0u16; 128];
        let n = GetWindowTextW(edit, b.as_mut_ptr(), 128).max(0) as usize;
        template_preview(&String::from_utf16_lossy(&b[..n]))
    }
    /// Live preview and plain-words problem for the template box.
    fn template_preview(text: &str) -> (String, String) {
        let app = crate::identity::name();
        match crate::file_prefs::check_template(text, &app) {
            Err(e) => (String::new(), e),
            Ok(()) => match crate::file_prefs::render_template(text, &app, local_time()) {
                Ok(b) => (format!("{b}.mp4"), String::new()),
                Err(e) => (String::new(), e),
            },
        }
    }
    fn save(exe: &Path, dir: &Path, data: Vec<u8>) -> Result<PathBuf, String> {
        // The note only exists when a saved template could not be used; 69's save() has no log handle.
        let (named, _note) = named_base(dir);
        let base = crate::file_prefs::unique_base(dir, &named);
        let partial = dir.join(format!("{base}.partial.mp4"));
        let out = dir.join(format!("{base}.mp4"));
        let mut p = engine(exe)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-y",
                "-i",
                "pipe:0",
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-c",
                "copy",
                "-avoid_negative_ts",
                "make_zero",
                "-movflags",
                "+faststart",
            ])
            .arg(&partial)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut input = p.stdin.take().unwrap();
        let writer = thread::spawn(move || input.write_all(&data));
        let result = p.wait_with_output().map_err(|e| e.to_string())?;
        let _ = writer.join();
        if !result.status.success() {
            let _ = fs::remove_file(&partial);
            return Err(String::from_utf8_lossy(&result.stderr).into_owned());
        }
        fs::rename(&partial, &out).map_err(|e| e.to_string())?;
        Ok(out)
    }
    fn attach_reader(
        p: &mut Child,
        ring: Arc<Mutex<Ring>>,
        log: Arc<Mutex<fs::File>>,
        tap: crate::recording::Shared,
    ) {
        let pid = p.id();
        writeln!(log.lock().unwrap(), "CAPTURE PROCESS pid={pid} starting").ok();
        let err = p.stderr.take().unwrap();
        let stderr_log = log.clone();
        thread::spawn(move || {
            let mut n = 0;
            for line in BufReader::new(err).lines() {
                let Ok(line) = line else { break };
                if n < 1024 * 1024 {
                    let is_progress = line.contains('=')
                        && [
                            "frame=",
                            "fps=",
                            "bitrate=",
                            "total_size=",
                            "out_time",
                            "dup_frames=",
                            "drop_frames=",
                            "speed=",
                            "progress=",
                            "stream_",
                        ]
                        .iter()
                        .any(|x| line.starts_with(x));
                    if !is_progress
                        || [
                            "frame=",
                            "dup_frames=",
                            "drop_frames=",
                            "out_time=",
                            "speed=",
                        ]
                        .iter()
                        .any(|x| line.starts_with(x))
                    {
                        writeln!(stderr_log.lock().unwrap(), "pid={pid} {line}").ok();
                        n += line.len();
                    }
                }
            }
        });
        let mut stream = p.stdout.take().unwrap();
        thread::spawn(move || {
            let mut fragment = Vec::new();
            loop {
                match read_box(&mut stream) {
                    Ok((typ, data)) => {
                        let mut r = ring.lock().unwrap();
                        match &typ {
                            b"ftyp" | b"moov" => r.init.extend_from_slice(&data),
                            b"moof" => fragment = data,
                            b"mdat" => {
                                if !fragment.is_empty() {
                                    fragment.extend_from_slice(&data);
                                    crate::recording::feed(&tap, &r.init, &fragment);
                                    r.push(std::mem::take(&mut fragment), Instant::now());
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        ring.lock().unwrap().error = Some(format!("Capture stopped: {e}"));
                        break;
                    }
                }
            }
        });
    }
    fn recent_files(dir: &Path) -> Vec<PathBuf> {
        let mut files: Vec<_> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("mp4"))
                    && !e.file_name().to_string_lossy().ends_with(".partial.mp4")
                    && !e
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".recoverable.mp4")
            })
            .collect();
        files.sort_by_key(|e| {
            (
                std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()),
                e.file_name(),
            )
        });
        crate::catalog::apply(files.into_iter().map(|e| e.path()).collect(), dir)
    }
    unsafe fn refresh_recent(dir: &Path, rows: &[HWND], paths: &mut Vec<PathBuf>, visible: bool) {
        let all = recent_files(dir);
        crate::library::state()
            .lock()
            .unwrap()
            .paths
            .retain(|p| p.is_file());
        let parent = GetParent(rows[0]);
        // Visibility is decided here, not by the caller: while Settings is shown the
        // library rows and layout must stay hidden whatever the caller passed.
        let visible = visible && GetPropW(parent, wide("display-settings").as_ptr()).is_null();
        let capacity = crate::library_view::capacity_for(parent);
        let count = all.len();
        {
            // Real library facts for the Home panel: every saved clip, newest first.
            let mut total = 0u64;
            let mut newest: Vec<(std::time::SystemTime, String, u64)> = vec![];
            let mut n = 0usize;
            for e in fs::read_dir(dir).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let lower = name.to_ascii_lowercase();
                if !lower.ends_with(".mp4") || lower.ends_with(".partial.mp4") || lower.ends_with(".recoverable.mp4") {
                    continue;
                }
                let Ok(m) = e.metadata() else { continue };
                n += 1;
                total += m.len();
                newest.push((m.modified().unwrap_or(std::time::UNIX_EPOCH), name[..name.len() - 4].to_string(), m.len()));
            }
            newest.sort_by(|a, b| b.0.cmp(&a.0));
            let now = std::time::SystemTime::now();
            let recent = newest
                .into_iter()
                .take(5)
                .map(|(t, name, len)| {
                    let age = now.duration_since(t).map(|d| d.as_secs()).unwrap_or(0);
                    (name, format!("{}, {}", crate::home_stats::age(age), crate::home_stats::size(len)))
                })
                .collect();
            if crate::home_stats::set_library(n, total, recent) {
                InvalidateRect(parent, std::ptr::null(), 0);
            }
        }
        EnableWindow(GetDlgItem(parent, 70), if count > 0 { 1 } else { 0 });
        let filters = crate::catalog::filter().lock().unwrap().clone();
        crate::library_view::refresh_tags(parent, dir);
        let offset = (GetPropW(parent, wide("library-offset").as_ptr()) as usize).min(
            count.saturating_sub(crate::library_view::capacity_for(parent)),
        );
        SetPropW(parent, wide("library-offset").as_ptr(), offset as _);
        SetPropW(parent, wide("library-count").as_ptr(), count as _);
        let bar = GetDlgItem(parent, 37);
        if !bar.is_null() {
            InvalidateRect(bar, std::ptr::null(), 0);
        }
        let hint = GetPropW(parent, wide("library-hint").as_ptr()) as HWND;
        if visible && !hint.is_null() {
            set(
                hint,
                &if count == 0 {
                    if filters.favourites && filters.query.trim().is_empty() && filters.tag.is_empty() {
                        "No favourites yet. Tap a clip's star to add one.".to_string()
                    } else if filters.query.trim().is_empty() && filters.tag.is_empty() {
                        // Polished mode paints its own illustration in the card.
                        if crate::polished::enabled() {
                            String::new()
                        } else {
                            "No clips. Save a replay to begin.".to_string()
                        }
                    } else {
                        "No matches. Clear filters.".to_string()
                    }
                } else {
                    format!(
                        "{}-{} of {}{}",
                        if count == 0 { 0 } else { offset + 1 },
                        (offset + capacity).min(count),
                        count,
                        [" / newest", " / oldest", " / longest", " / shortest"]
                            [filters.sort.min(3)]
                    )
                },
            );
            let empty_illustration = crate::polished::enabled()
                && count == 0
                && filters.query.trim().is_empty()
                && filters.tag.is_empty()
                && !filters.favourites;
            SetPropW(
                hint,
                wide("polished-empty").as_ptr(),
                if empty_illustration {
                    1usize as _
                } else {
                    std::ptr::null_mut()
                },
            );
            InvalidateRect(hint, std::ptr::null(), 0);
        }
        {
            let home: Vec<PathBuf> = all.iter().take(4).cloned().collect();
            for i in 0..4 {
                let c = GetDlgItem(parent, 120 + i);
                if c.is_null() {
                    continue;
                }
                if let Some(p) = home.get(i as usize) {
                    crate::ui::bind_thumb(c, p.clone());
                    let mb = fs::metadata(p).map(|m| m.len() as f64 / 1_000_000.0).unwrap_or(0.0);
                    set(c, &format!("{}\n{mb:.1} MB", p.file_stem().unwrap_or_default().to_string_lossy()));
                }
            }
            crate::home_stats::set_home_paths(home);
        }
        *paths = all.into_iter().skip(offset).take(capacity).collect();
        SetPropW(parent, wide("library-filled").as_ptr(), (paths.len() + 1) as _);
        for (i, row) in rows.iter().enumerate() {
            if let Some(p) = paths.get(i) {
                crate::ui::bind_thumb(*row, p.clone());
                let mb = fs::metadata(p)
                    .map(|m| m.len() as f64 / 1_000_000.0)
                    .unwrap_or(0.0);
                set(
                    *row,
                    &format!(
                        "{}\n{}\n{}",
                        p.file_stem().unwrap_or_default().to_string_lossy(),
                        format!("{mb:.1} MB"),
                        {
                            let d = crate::clip_date::modified_label(p);
                            if d.is_empty() { d } else { format!("Modified {d}") }
                        }
                    ),
                );
                ShowWindow(*row, if visible { SW_SHOW } else { SW_HIDE });
            } else {
                ShowWindow(*row, SW_HIDE);
            }
        }
        crate::replay_design::home_rows(parent);
        if visible {
            crate::library_view::layout(
                parent,
                GetPropW(parent, wide("display-layout").as_ptr()) as usize,
                rows,
                GetPropW(parent, wide("library-large").as_ptr()) as usize != 0,
            );
        }
    }
    unsafe fn layout_replays(rows: &[HWND], large: bool) {
        for (i, row) in rows.iter().enumerate() {
            SetWindowPos(
                *row,
                std::ptr::null_mut(),
                630,
                180 + i as i32 * if large { 72 } else { 58 },
                274,
                if large { 64 } else { 38 },
                SWP_NOZORDER,
            );
            InvalidateRect(*row, std::ptr::null(), 0);
        }
    }
    unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize {
        if let Some(result) = crate::background::handle(hwnd, msg, l) {
            return result;
        }
        if let Some(result) = crate::ui::handle(hwnd, msg, w, l) {
            return result;
        }
        match msg {
            WM_MOUSEWHEEL => {
                if !GetPropW(hwnd, wide("display-settings").as_ptr()).is_null() {
                    crate::settings_view::scroll(if (w >> 16) as i16 > 0 { -96 } else { 96 });
                    return 0;
                }
                let x = l as i16 as i32;
                let y = (l >> 16) as i16 as i32;
                let mut p = POINT { x, y };
                ScreenToClient(hwnd, &mut p);
                let mode = GetPropW(hwnd, wide("display-layout").as_ptr()) as usize;
                let in_library = match mode {
                    1 | 3 if crate::nav::compact_page(mode, crate::polished::enabled(), false) == Some(1) => {
                        // Clips page of Compact/Stacked: the list starts under the filters.
                        p.x >= 24 && p.y >= 150 && p.y < 622
                    }
                    1 => p.x >= 24 && p.y >= 372 && p.y < 590,
                    2 => p.x >= 396 && p.y >= 88 && p.y < 602,
                    3 => p.x >= 24 && p.y >= 350 && p.y < 622,
                    4 => p.x >= crate::polished::SB + 16 && p.y >= 88 && p.y < 622,
                    _ => p.x >= 490 && p.x < 948 && p.y >= 88 && p.y < 622,
                };
                let mut wheel_client = RECT::default();
                GetClientRect(hwnd, &mut wheel_client);
                let in_library = in_library && p.y < crate::layout_plan::content_bottom(wheel_client.bottom);
                if in_library && IsWindowVisible(GetDlgItem(hwnd, 100)) != 0 {
                    PostThreadMessageW(
                        GetCurrentThreadId(),
                        WM_HOTKEY,
                        if (w >> 16) as i16 > 0 { 37 } else { 38 },
                        0,
                    );
                    return 0;
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_HSCROLL => {
                let id = GetDlgCtrlID(l as HWND);
                if id as usize == crate::ctl_ids::REPLAY_SLIDER {
                    // Dragging only updates the read-out; the length is applied when the thumb is released.
                    let p = SendMessageW(l as HWND, 0x400, 0, 0) as u64;
                    let v = ((p + 2) / 5 * 5).clamp(5, 300);
                    let value = GetDlgItem(hwnd, 42);
                    let mut t = [0u16; 16];
                    let txt: Vec<u16> = format!("{v}s").encode_utf16().chain(Some(0)).collect();
                    t[..txt.len()].copy_from_slice(&txt);
                    SetWindowTextW(value, t.as_ptr());
                    if (w & 0xffff) != 8 {
                        return 0;
                    }
                }
                PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, id as usize, 0);
                0
            }
            WM_COMMAND => {
                let id = w & 0xffff;
                if (id == 33 || id == crate::ctl_ids::PAUSE_KEY) && ((w >> 16) & 0xffff) == 0x100 {
                    return 0;
                }
                // The template box previews while typing and applies only from its button.
                if id == crate::ctl_ids::TEMPLATE_EDIT {
                    if ((w >> 16) & 0xffff) == EN_CHANGE as usize {
                        PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, crate::ctl_ids::TEMPLATE_PREVIEW, 0);
                    }
                    return 0;
                }
                if [20, 21, 22, 23, 31, 45, 77, 78].contains(&id)
                    && ((w >> 16) & 0xffff) != CBN_SELCHANGE as usize
                {
                    return 0;
                }

                if id == 76 && (w >> 16) != EN_CHANGE as usize {
                    return 0;
                }
                // The inline app name saves when the box loses focus (Enter moves focus away).
                if (id == 95 || id == crate::ctl_ids::TAGLINE_EDIT) && (w >> 16) != 0x200 {
                    return 0;
                }
                PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, w & 0xffff, 0);
                0
            }
            WM_CLOSE => {
                let hide = crate::file_prefs::close_hides_to_tray(
                    crate::file_prefs::prefs_file().parent().unwrap_or(std::path::Path::new(".")),
                );
                if hide && !GetPropW(hwnd, wide("tray-ready").as_ptr()).is_null() {
                    ShowWindow(hwnd, SW_HIDE);
                } else {
                    PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 3, 0);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
    unsafe fn label(hwnd: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, font: HFONT) -> HWND {
        let c = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE,
            x,
            y,
            w,
            h,
            hwnd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        SendMessageW(c, WM_SETFONT, font as usize, 1);
        crate::ui::stable_label(c);
        c
    }
    unsafe fn button(
        hwnd: HWND,
        text: &str,
        id: usize,
        x: i32,
        y: i32,
        w: i32,
        font: HFONT,
    ) -> HWND {
        let c = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            x,
            y,
            w,
            38,
            hwnd,
            id as _,
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        crate::ui::interactive(c);
        SendMessageW(c, WM_SETFONT, font as usize, 1);
        c
    }
    unsafe fn combo(
        hwnd: HWND,
        id: usize,
        x: i32,
        y: i32,
        w: i32,
        items: &[&str],
        font: HFONT,
    ) -> HWND {
        let c = CreateWindowExW(
            0,
            wide("COMBOBOX").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | CBS_DROPDOWNLIST as u32
                | CBS_OWNERDRAWFIXED as u32
                | CBS_HASSTRINGS as u32,
            x,
            y,
            w,
            380,
            hwnd,
            id as _,
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        SendMessageW(c, WM_SETFONT, font as usize, 1);
        for s in items {
            SendMessageW(c, CB_ADDSTRING, 0, wide(s).as_ptr() as isize);
        }
        SendMessageW(c, CB_SETCURSEL, 0, 0);
        crate::ui::dropdown(c);
        c
    }
    unsafe fn slider(hwnd: HWND, id: usize, x: i32, y: i32, w: i32) -> HWND {
        let c = CreateWindowExW(
            0,
            wide("msctls_trackbar32").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            x,
            y,
            w,
            32,
            hwnd,
            id as _,
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        SendMessageW(c, TBM_SETRANGEMAX, 1, 24);
        crate::ui::modern_slider(c);
        c
    }
    /// Density button window text. Polished draws icons (plain accessible names);
    /// Legacy shows text with a check mark on the selected one. Always refreshed.
    unsafe fn density_labels(largebutton: HWND, sizebutton: HWND, large: bool) {
        if crate::polished::enabled() {
            set(largebutton, "Larger");
            set(sizebutton, "Compact");
        } else {
            set(largebutton, if large { "\u{2713} Larger" } else { "Larger" });
            set(sizebutton, if large { "Compact" } else { "\u{2713} Compact" });
        }
    }
    /// Show the replay length everywhere it appears: slider position, value button and the active quick button.
    unsafe fn sync_len(slider: HWND, value: HWND, quick: &[HWND; 4], secs: u64) {
        SendMessageW(slider, 0x405, 1, secs as isize); // TBM_SETPOS
        InvalidateRect(slider, std::ptr::null(), 0);
        UpdateWindow(slider);
        set(value, &format!("{secs}s"));
        for (i, q) in quick.iter().enumerate() {
            let on = crate::ctl_ids::QUICK_SECS[i] == secs;
            SetPropW(*q, wide("section-active").as_ptr(), if on { 1usize as _ } else { std::ptr::null_mut() });
            InvalidateRect(*q, std::ptr::null(), 0);
        }
    }
    unsafe fn set(hwnd: HWND, s: &str) {
        let mut old = vec![0u16; (GetWindowTextLengthW(hwnd) + 1).max(1) as usize];
        let n = GetWindowTextW(hwnd, old.as_mut_ptr(), old.len() as i32);
        if String::from_utf16_lossy(&old[..n as usize]) != s {
            SetWindowTextW(hwnd, wide(s).as_ptr());
        }
    }
    pub fn run() -> Result<(), String> {
        unsafe {
            SetProcessDPIAware();
            crate::ui::install_select_all();
            let instance = CreateMutexW(std::ptr::null(), 0, wide("Local\\ZimClip").as_ptr());
            if instance.is_null() {
                return Err("Cannot create app lock.".into());
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(instance);
                let h = FindWindowW(wide("ZimClipWindow").as_ptr(), std::ptr::null());
                if !h.is_null() {
                    crate::background::reveal(h);
                }
                return Ok(());
            }
            let root = std::env::current_exe()
                .map_err(|e| e.to_string())?
                .parent()
                .unwrap()
                .to_owned();
            // Before any settings read or capture start: undo an interrupted settings import.
            let mut startup_blocked = false;
            let startup_note: Option<String> = match crate::settings_bundle::recover(&root) {
                crate::settings_bundle::Recovery::Clean => None,
                crate::settings_bundle::Recovery::Restored => Some("An unfinished settings import was undone. Your earlier settings are back.".to_string()),
                crate::settings_bundle::Recovery::Blocked(m) => {
                    startup_blocked = true;
                    Some(m)
                }
            };
            crate::polished::load(&root);
            if !std::env::args().any(|x| x == "--background") {
                crate::welcome::show(&root);
            }
            crate::comms::load(&root);
            crate::audio_devices::load(&root);
            let exe = root.join("capture-engine.exe");
            if !exe.exists() {
                return Err("capture-engine.exe is missing. Extract the entire ZIP first.".into());
            }
            let mut dir = crate::save_folder::load(&root);
            fs::create_dir_all(&dir)
                .map_err(|e| format!("Cannot create Clips: {e}. Extract to a writable folder."))?;
            let log = Arc::new(Mutex::new(
                fs::File::create(root.join("diagnostics.txt")).map_err(|e| e.to_string())?,
            ));
            writeln!(
                log.lock().unwrap(),
                concat!(
                    "ZimClip ",
                    env!("CARGO_PKG_VERSION"),
                    " | Windows x64 | system audio | AMF H.264 4Mbps | selectable 30/60fps CFR"
                )
            )
            .ok();
            let class = wide("ZimClipWindow");
            let hinst = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: hinst,
                lpszClassName: class.as_ptr(),
                hbrBackground: CreateSolidBrush(crate::ui::bg()),
                hIcon: LoadIconW(hinst, 1usize as _),
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&wc) == 0 {
                return Err("Cannot register app window.".into());
            }
            let hwnd = CreateWindowExW(
                WS_EX_COMPOSITED,
                class.as_ptr(),
                wide(&format!(
                    "ZimClip {} | Desktop replay",
                    env!("CARGO_PKG_VERSION")
                ))
                .as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                970,
                740,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return Err("Cannot create app window.".into());
            }
            crate::ui::titlebar(hwnd);
            let stored_accent = fs::read_to_string(root.join("appearance.ini"))
                .unwrap_or_default()
                .trim()
                .parse::<usize>()
                .unwrap_or(0);
            crate::ui::set_accent(stored_accent);
            crate::startup_note(&format!("Loaded accent preference {stored_accent}"));
            crate::display::load(&root);
            let font = CreateFontW(
                -14,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                wide(&crate::display::font_name()).as_ptr(),
            );
            let titlefont = CreateFontW(
                -24,
                0,
                0,
                0,
                600,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                wide(&crate::display::font_name()).as_ptr(),
            );
            let dc = GetDC(hwnd);
            let old = SelectObject(dc, font as _);
            let mut resolved = [0u16; 100];
            let n = GetTextFaceW(dc, 100, resolved.as_mut_ptr());
            crate::startup_note(&format!(
                "UI font requested={} resolved={}",
                crate::display::font_name(),
                String::from_utf16_lossy(&resolved[..n.max(0) as usize]).trim_end_matches('\0')
            ));
            SelectObject(dc, old);
            ReleaseDC(hwnd, dc);
            crate::identity::load(&root);
            crate::identity::load_icon(&root);
            crate::identity::load_tagline(&root);
            let appheading = label(hwnd, &crate::identity::name(), 66, 20, 608, 42, titlefont);
            crate::ui::apply_identity(hwnd, false);
            button(hwnd, "Settings", 14, 736, 24, 192, font);
            // Sidebar navigation (Polished Sidebar layout only; hidden otherwise).
            for (id, name) in [(110, "Home"), (111, "Clips"), (112, "Editor"), (113, "Settings")] {
                let b = button(hwnd, name, id, 12, 96, 132, font);
                ShowWindow(b, SW_HIDE);
            }
            let arrange = button(hwnd, "Arrange Home", 114, 600, 18, 130, font);
            crate::ui::tooltip(arrange, "Show, hide or reorder Home cards, or reset them");
            ShowWindow(arrange, SW_HIDE);
            let subswitch = button(hwnd, "Audio sources  >", crate::ctl_ids::HOME_SUBPAGE, 600, 18, 130, font);
            ShowWindow(subswitch, SW_HIDE);
            let modebutton = button(
                hwnd,
                if crate::polished::enabled() {
                    "Legacy UI"
                } else {
                    "Polished UI"
                },
                89,
                580,
                24,
                192,
                font,
            );
            crate::ui::tooltip(
                modebutton,
                "Switch interface style. Polished is the reference design; Legacy uses the old, lighter interface.",
            );
            let accentbutton = button(hwnd, "Accent: Mint", 11, 736, 24, 192, font);
            ShowWindow(accentbutton, SW_HIDE);
            let tagline = label(
                hwnd,
                &crate::identity::tagline(),
                26,
                65,
                560,
                24,
                font,
            );
            SetWindowLongPtrW(tagline, GWL_STYLE, GetWindowLongPtrW(tagline, GWL_STYLE) | 0x4000); // SS_ENDELLIPSIS
            let viewheading = label(hwnd, "LIVE BUFFER", 48, 122, 300, 24, font);
            let capturebutton = button(hwnd, "Pause replay", 10, 386, 112, 176, font);
            let status = label(hwnd, "Starting capture...", 48, 156, 510, 30, font);
            let detail = label(
                hwnd,
                "Checking GPU conversion. CPU fallback is automatic.",
                48,
                196,
                510,
                40,
                font,
            );
            let mut buffer_seconds = fs::read_to_string(root.join("buffer-seconds.txt"))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .filter(|v| (5..=300).contains(v))
                .unwrap_or(30);
            let savebutton = button(
                hwnd,
                "Save last 30 seconds     Alt+G",
                1,
                48,
                242,
                514,
                font,
            );
            EnableWindow(savebutton, 0);
            let setupheading = label(hwnd, "CAPTURE SETUP", 48, 310, 500, 24, font);
            let audiobutton = button(hwnd, "System audio ON", 7, 48, 346, 248, font);
            let gainbutton = button(hwnd, "Boost +0dB", 6, 310, 346, 252, font);
            let resbutton = button(hwnd, "720p max", 8, 48, 396, 248, font);
            let pathbutton = button(hwnd, "GPU conversion", 9, 310, 396, 252, font);
            let micbutton = button(hwnd, "Mic OFF", 12, 48, 466, 248, font);
            let micgainbutton = button(hwnd, "Mic +0dB", 13, 310, 466, 252, font);
            let audiolabel = label(hwnd, "Starting audio...", 48, 520, 514, 70, font);
            let rightheading = label(hwnd, "RECENT CLIPS", 630, 122, 170, 24, font);
            let mut recentrows = Vec::new();
            for i in 0..5 {
                recentrows.push(crate::ui::replay_card(hwnd, 100 + i, font));
            }
            // Sidebar Home: the newest clips as real cards (thumbnail, click to open) and a
            // "View all" link to the Clips page. Hidden until the Home layout shows them.
            for i in 0..4 {
                let c = crate::ui::replay_card(hwnd, 120 + i, font);
                ShowWindow(c, SW_HIDE);
            }
            let viewall = button(hwnd, "View all", 125, 700, 104, 80, font);
            ShowWindow(viewall, SW_HIDE);
            let mut large_cards = fs::read_to_string(root.join("library.ini"))
                .unwrap_or_else(|_| "large=1".into())
                .trim()
                != "large=0";
            let sizebutton = button(
                hwnd,
                "Compact",
                36,
                806,
                116,
                74,
                font,
            );
            let largebutton = button(hwnd, "Larger", 90, 708, 116, 98, font);
            density_labels(largebutton, sizebutton, large_cards);
            crate::ui::tooltip(largebutton, "Large clip cards");
            crate::ui::tooltip(sizebutton, "Compact clip rows");
            SetWindowPos(
                sizebutton,
                std::ptr::null_mut(),
                806,
                116,
                98,
                30,
                SWP_NOZORDER,
            );
            layout_replays(&recentrows, large_cards);
            let libraryscroll = crate::ui::replay_scroll(hwnd, 910, 180, 10, 352);
            let (thumbtx, thumbrx) = mpsc::channel();
            let mut recentpaths = Vec::new();
            refresh_recent(&dir, &recentrows, &mut recentpaths, true);
            crate::thumbs::request(recentpaths.clone(), thumbtx.clone());
            let righthint = label(hwnd, "Click a replay to open it.", 630, 154, 268, 24, font);
            SetPropW(hwnd, wide("library-hint").as_ptr(), righthint as _);
            refresh_recent(&dir, &recentrows, &mut recentpaths, true);
            let openclipsbutton = button(hwnd, "Open Clips", 2, 630, 550, 274, font);
            let filterclips = crate::library_view::toolbar(hwnd, font);
            let selectclips = button(hwnd, "Select clips", 70, 788, 550, 116, font);
            let deleteclips = button(hwnd, "Delete selected", 71, 630, 510, 274, font);
            ShowWindow(deleteclips, SW_HIDE);
            let result = label(
                hwnd,
                startup_note.as_deref().unwrap_or("Starting capture. Saved clips stay available."),
                26,
                618,
                562,
                40,
                font,
            );
            let restartbutton = button(hwnd, "Restart", 4, 606, 618, 142, font);
            EnableWindow(restartbutton, 0);
            button(hwnd, "Diagnostics", 5, 762, 618, 166, font);
            let refreshbutton = button(hwnd, "Refresh", 81, 630, 518, 100, font);
            button(hwnd, "Tags", 85, 738, 518, 60, font);
            crate::ui::tooltip(
                refreshbutton,
                "Reload this video folder without restarting capture",
            );
            let mut settings_controls = Vec::new();
            for (s, y) in [
                ("Video changes reset the unsaved buffer.", 142),
                ("Resolution", 180),
                ("Capture path", 264),
                ("System gain", 394),
                ("Microphone gain", 464),
            ] {
                settings_controls.push(label(
                    hwnd,
                    s,
                    48,
                    y,
                    if y == 142 { 514 } else { 196 },
                    24,
                    font,
                ));
            }
            let rescombo = combo(
                hwnd,
                20,
                48,
                210,
                330,
                &["480p max", "720p max", "1080p max"],
                font,
            );
            SendMessageW(rescombo, CB_SETCURSEL, 1, 0);
            let fpslabel = label(hwnd, "Capture FPS", 390, 182, 172, 24, font);
            let fpscombo = combo(hwnd, 23, 390, 210, 172, &["30 FPS", "60 FPS (test)"], font);
            settings_controls.extend([fpslabel, fpscombo]);
            let pathcombo = combo(
                hwnd,
                21,
                48,
                294,
                514,
                &[
                    "CPU compatibility (recommended)",
                    "GPU conversion (experimental)",
                ],
                font,
            );
            let accentcombo = combo(
                hwnd,
                22,
                630,
                294,
                274,
                &[
                    "Mint", "Sky", "Lilac", "Amber", "Ice", "Peach", "Rose", "Silver",
                ],
                font,
            );
            SendMessageW(accentcombo, CB_SETCURSEL, stored_accent % 8, 0);
            let sysslider = slider(hwnd, 26, 48, 424, 392);
            let micslider = slider(hwnd, 27, 48, 494, 392);
            let sysvalue = label(hwnd, "+0 dB", 458, 430, 100, 24, font);
            let micvalue = label(hwnd, "+0 dB", 458, 500, 100, 24, font);
            settings_controls.extend([
                rescombo,
                pathcombo,
                accentcombo,
                sysslider,
                micslider,
                sysvalue,
                micvalue,
            ]);
            for c in &settings_controls {
                ShowWindow(*c, SW_HIDE);
            }
            for c in [
                setupheading,
                gainbutton,
                micgainbutton,
                resbutton,
                pathbutton,
                accentbutton,
            ] {
                ShowWindow(c, SW_HIDE);
            }
            SetWindowPos(
                audiobutton,
                std::ptr::null_mut(),
                48,
                346,
                248,
                38,
                SWP_NOZORDER,
            );
            SetWindowPos(
                micbutton,
                std::ptr::null_mut(),
                310,
                346,
                252,
                38,
                SWP_NOZORDER,
            );
            SetWindowPos(
                audiolabel,
                std::ptr::null_mut(),
                48,
                350,
                514,
                170,
                SWP_NOZORDER,
            );
            let privacy = label(
                hwnd,
                "Stop recording before pausing or changing capture.",
                48,
                558,
                514,
                24,
                font,
            );
            settings_controls.extend([audiobutton, micbutton]);
            ShowWindow(audiobutton, SW_HIDE);
            ShowWindow(micbutton, SW_HIDE);
            let detailsbutton = button(hwnd, "Details", 40, 48, 510, 150, font);
            let simplehint = label(
                hwnd,
                "Your last 30 seconds, ready when you are.",
                48,
                230,
                514,
                24,
                font,
            );
            let warning = label(hwnd, "", 48, 392, 514, 74, font);
            let bufferbar = label(hwnd, "0/30", 48, 232, 400, 30, font);
            SetPropW(bufferbar, wide("replay-buffer").as_ptr(), 1usize as _);
            let sourceslabel = label(hwnd, "AUDIO SOURCES", 48, 328, 400, 20, font);
            let sourcenote = label(
                hwnd,
                "Changing a source restarts capture and clears the unsaved replay.",
                48,
                520,
                410,
                38,
                font,
            );
            let encoderlabel = label(hwnd, "Starting capture...", 48, 200, 410, 24, font);
            SendMessageW(simplehint, WM_SETFONT, titlefont as usize, 1);
            SendMessageW(status, WM_SETFONT, font as usize, 1);
            SetWindowPos(status, std::ptr::null_mut(), 48, 182, 514, 36, SWP_NOZORDER);
            SendMessageW(status, WM_SETFONT, font as usize, 1);
            SetWindowPos(
                savebutton,
                std::ptr::null_mut(),
                48,
                274,
                514,
                44,
                SWP_NOZORDER,
            );
            SetWindowPos(
                capturebutton,
                std::ptr::null_mut(),
                218,
                332,
                176,
                38,
                SWP_NOZORDER,
            );
            SetWindowPos(detail, std::ptr::null_mut(), 48, 390, 514, 48, SWP_NOZORDER);
            SetWindowPos(
                audiolabel,
                std::ptr::null_mut(),
                48,
                442,
                514,
                64,
                SWP_NOZORDER,
            );
            ShowWindow(detail, SW_HIDE);
            ShowWindow(audiolabel, SW_HIDE);
            let mut details_open = false;
            SetWindowPos(
                capturebutton,
                std::ptr::null_mut(),
                48,
                332,
                248,
                38,
                SWP_NOZORDER,
            );
            let recordbutton = button(hwnd, "Start recording", 43, 310, 332, 252, font);
            let recoverbutton = button(hwnd, "Recover recording", 44, 310, 510, 252, font);
            crate::nav::set_recover(crate::recording::pending(&dir));
            ShowWindow(
                recoverbutton,
                if crate::recording::pending(&dir) {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
            let main_controls = [
                viewheading,
                recordbutton,
                capturebutton,
                status,
                savebutton,
                detailsbutton,
                simplehint,
                warning,
                privacy,
                bufferbar,
                sourceslabel,
                sourcenote,
                encoderlabel,
            ];
            let updatebutton = button(hwnd, "Check / download update", 30, 48, 550, 514, font);
            ShowWindow(updatebutton, SW_HIDE);
            settings_controls.push(updatebutton);
            let prefs = fs::read_to_string(root.join("personalization.ini")).unwrap_or_default();
            let pref = |key: &str, default: usize| {
                prefs
                    .lines()
                    .find_map(|s| {
                        s.split_once('=')
                            .filter(|(k, _)| *k == key)
                            .and_then(|(_, v)| v.parse::<usize>().ok())
                    })
                    .unwrap_or(default)
            };
            let themelabel = label(hwnd, "Canvas theme", 630, 190, 274, 24, font);
            let themecombo = combo(
                hwnd,
                31,
                630,
                220,
                274,
                &[
                    "Charcoal",
                    "Midnight",
                    "Plum",
                    "Ember",
                    "Forest",
                    "High contrast",
                    "Custom",
                ],
                font,
            );
            crate::custom_theme::load(&root);
            crate::nav::load_cards(&root);
            let theme_index = pref("theme", 0) % 7;
            crate::ui::set_theme(theme_index);
            SendMessageW(themecombo, CB_SETCURSEL, theme_index, 0);
            let accentlabel = label(hwnd, "Accent color", 630, 266, 274, 24, font);
            settings_controls.push(accentlabel);
            let shortcuttitle = label(hwnd, "Save shortcut", 630, 342, 274, 24, font);
            let hotkey = CreateWindowExW(
                0,
                wide("msctls_hotkey32").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER,
                630,
                370,
                274,
                30,
                hwnd,
                33usize as _,
                hinst,
                std::ptr::null(),
            );
            SendMessageW(hotkey, WM_SETFONT, font as usize, 1);
            SendMessageW(hotkey, 0x401, pref("hotkey", 0x447), 0);
            crate::ui::modern_hotkey(hotkey);
            let applykey = button(hwnd, "Apply save shortcut", 32, 630, 410, 274, font);
            let prefs_dir0 = crate::file_prefs::prefs_file().parent().map(|d| d.to_path_buf()).unwrap_or_default();
            let mut pause_packed = crate::file_prefs::pause_key(&prefs_dir0);
            let pausetitle = label(hwnd, "Pause shortcut", 630, 462, 274, 24, font);
            let pausekey = CreateWindowExW(
                0,
                wide("msctls_hotkey32").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER,
                630,
                490,
                274,
                30,
                hwnd,
                crate::ctl_ids::PAUSE_KEY as _,
                hinst,
                std::ptr::null(),
            );
            SendMessageW(pausekey, WM_SETFONT, font as usize, 1);
            SendMessageW(pausekey, 0x401, pause_packed as usize, 0);
            crate::ui::modern_hotkey(pausekey);
            let applypause = button(hwnd, "Apply pause shortcut", crate::ctl_ids::APPLY_PAUSE, 630, 530, 274, font);
            let mut sound_on = pref("sound", 1) != 0;
            let mut toast_on = pref("toast", 1) != 0;
            let soundbutton = button(
                hwnd,
                if sound_on {
                    "Save ding ON"
                } else {
                    "Save ding OFF"
                },
                34,
                630,
                466,
                274,
                font,
            );
            let toastbutton = button(
                hwnd,
                if toast_on {
                    "Clip notification ON"
                } else {
                    "Clip notification OFF"
                },
                35,
                630,
                514,
                274,
                font,
            );
            let motionbutton = button(
                hwnd,
                if crate::ui::motion::reduced() { "Motion: reduced" } else { "Motion: full" },
                91,
                630,
                606,
                274,
                font,
            );
            crate::ui::tooltip(
                motionbutton,
                "Reduce motion turns off the hover, press and page transitions. Windows animation settings are also honoured.",
            );
            let customthemebutton = button(hwnd, "Custom...", 92, 790, 188, 114, font);
            crate::ui::tooltip(
                customthemebutton,
                "Pick background, panel and text colours, save named presets, or reset to a built-in theme.",
            );
            let feedbackhint = label(
                hwnd,
                "Sound may enter later clips.\nToast never takes keyboard focus.",
                630,
                562,
                274,
                40,
                font,
            );
            let identitybutton = button(hwnd, "Change icon", 41, 630, 562, 274, font);
            let nameedit = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide(&crate::identity::name()).as_ptr(),
                WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                630,
                520,
                274,
                32,
                hwnd,
                95usize as _,
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            SendMessageW(nameedit, WM_SETFONT, font as usize, 1);
            SendMessageW(nameedit, EM_LIMITTEXT, 24, 0);
            crate::ui::search_field(nameedit);
            settings_controls.push(nameedit);
            ShowWindow(nameedit, SW_HIDE);
            let taglineedit = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide(&crate::identity::tagline()).as_ptr(),
                WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                630,
                520,
                274,
                32,
                hwnd,
                crate::ctl_ids::TAGLINE_EDIT as _,
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            SendMessageW(taglineedit, WM_SETFONT, font as usize, 1);
            SendMessageW(taglineedit, EM_LIMITTEXT, crate::identity::MAX_TAGLINE, 0);
            crate::ui::search_field(taglineedit);
            settings_controls.push(taglineedit);
            ShowWindow(taglineedit, SW_HIDE);
            let durationbutton = button(hwnd, "Replay length", 42, 218, 550, 156, font);
            set(updatebutton, "Check update");
            SetWindowPos(
                updatebutton,
                std::ptr::null_mut(),
                48,
                550,
                156,
                38,
                SWP_NOZORDER,
            );
            settings_controls.push(durationbutton);
            // Replay length: a slider (5-300 s, steps of 5), the value button (opens the exact box)
            // and four quick lengths. Changes go through the same guarded apply as before.
            let lenslider = slider(hwnd, crate::ctl_ids::REPLAY_SLIDER, 218, 520, 156);
            SendMessageW(lenslider, 0x407, 0, 5); // TBM_SETRANGEMIN
            SendMessageW(lenslider, 0x408, 1, 300); // TBM_SETRANGEMAX
            SendMessageW(lenslider, 0x417, 0, 5); // TBM_SETLINESIZE
            SendMessageW(lenslider, 0x415, 0, 30); // TBM_SETPAGESIZE
            settings_controls.push(lenslider);
            let mut lenquick = [std::ptr::null_mut(); 4];
            for (i, label) in ["15 s", "30 s", "1 min", "2 min"].iter().enumerate() {
                lenquick[i] = button(hwnd, label, crate::ctl_ids::QUICK_LEN[i], 218, 580, 60, font);
                settings_controls.push(lenquick[i]);
            }
            ShowWindow(feedbackhint, SW_HIDE);
            settings_controls.extend([
                identitybutton,
                motionbutton,
                customthemebutton,
                themelabel,
                themecombo,
                shortcuttitle,
                hotkey,
                applykey,
                pausetitle,
                pausekey,
                applypause,
                soundbutton,
                toastbutton,
            ]);
            for c in &settings_controls {
                ShowWindow(*c, SW_HIDE);
            }
            let layoutcombo = combo(
                hwnd,
                45,
                48,
                650,
                326,
                &[
                    "Balanced split",
                    "Compact capture",
                    "Wide library",
                    "Stacked studio",
                    "Sidebar (Polished UI)",
                ],
                font,
            );
            let layoutlabel = label(hwnd, "Layout", 48, 622, 326, 24, font);
            let fontbutton = button(hwnd, "App font...", 46, 630, 650, 274, font);
            let fontlabel = label(hwnd, "", 630, 622, 274, 24, font);
            settings_controls.extend([layoutcombo, layoutlabel, fontbutton, fontlabel]);
            for c in [layoutcombo, layoutlabel, fontbutton, fontlabel] {
                ShowWindow(c, SW_HIDE);
            }
            let mut layout = fs::read_to_string(root.join("layout.txt"))
                .unwrap_or_default()
                .trim()
                .parse::<usize>()
                .unwrap_or(0)
                .min(4);
            SendMessageW(layoutcombo, CB_SETCURSEL, layout, 0);
            let (update_tx, update_rx) = mpsc::channel();
            let (update_progress_tx, update_progress_rx) = mpsc::channel();
            let mut checking_update = false;
            // Update notice: one quiet manifest check shortly after open, off the UI thread.
            let (notice_tx, notice_rx) = mpsc::channel::<String>();
            {
                let root = root.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs(30));
                    if let Ok(Some(v)) = crate::updater::peek() {
                        if crate::updater::notice_due(&root, &v) {
                            notice_tx.send(v).ok();
                        }
                    }
                });
            }
            let mut pending_notice: Option<String> = None;
            let mut install_when_staged = false;
            let mut staged_update: Option<crate::updater::Staged> = None;
            let mut auto_start = crate::background::startup_enabled();
            let startupbutton = button(
                hwnd,
                if auto_start {
                    "Start with Windows ON"
                } else {
                    "Start with Windows OFF"
                },
                62,
                338,
                550,
                224,
                font,
            );
            set(updatebutton, "Update");
            SetWindowPos(
                updatebutton,
                std::ptr::null_mut(),
                48,
                550,
                130,
                38,
                SWP_NOZORDER,
            );
            SetWindowPos(
                durationbutton,
                std::ptr::null_mut(),
                192,
                550,
                132,
                38,
                SWP_NOZORDER,
            );
            settings_controls.push(startupbutton);
            ShowWindow(startupbutton, SW_HIDE);
            let mut confirm_deletes = !crate::file_prefs::delete_confirm_skipped(&crate::file_prefs::prefs_file());
            let confirmbutton = button(
                hwnd,
                if confirm_deletes { "Confirm before deleting ON" } else { "Confirm before deleting OFF" },
                63,
                48,
                750,
                256,
                font,
            );
            settings_controls.push(confirmbutton);
            ShowWindow(confirmbutton, SW_HIDE);
            let sharebutton = button(
                hwnd,
                &crate::file_prefs::share_label(crate::file_prefs::prefs_file().parent()),
                64,
                48,
                800,
                256,
                font,
            );
            settings_controls.push(sharebutton);
            ShowWindow(sharebutton, SW_HIDE);
            let mut close_hides = crate::file_prefs::close_hides_to_tray(
                crate::file_prefs::prefs_file().parent().unwrap_or(std::path::Path::new(".")),
            );
            let closebutton = button(hwnd, crate::file_prefs::close_label(close_hides), 66, 306, 850, 256, font);
            settings_controls.push(closebutton);
            ShowWindow(closebutton, SW_HIDE);
            let namedir = crate::file_prefs::prefs_file().parent().map(|d| d.to_path_buf()).unwrap_or_default();
            let namebutton = button(hwnd, crate::file_prefs::name_label(crate::file_prefs::name_style(&namedir)), 97, 306, 900, 256, font);
            settings_controls.push(namebutton);
            ShowWindow(namebutton, SW_HIDE);
            crate::settings_view::set_name_example(&name_example(&namedir));
            let tpledit = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide(&crate::file_prefs::template(&namedir).unwrap_or_else(|| "{app}-{date}-{time}".to_string())).as_ptr(),
                WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                306,
                950,
                256,
                32,
                hwnd,
                crate::ctl_ids::TEMPLATE_EDIT as _,
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            SendMessageW(tpledit, WM_SETFONT, font as usize, 1);
            SendMessageW(tpledit, EM_LIMITTEXT, 80, 0);
            crate::ui::search_field(tpledit);
            let tplapply = button(hwnd, "Apply template", crate::ctl_ids::APPLY_TEMPLATE, 306, 950, 130, font);
            settings_controls.extend([tpledit, tplapply]);
            ShowWindow(tpledit, SW_HIDE);
            ShowWindow(tplapply, SW_HIDE);
            {
                let mut b = [0u16; 128];
                let n = GetWindowTextW(tpledit, b.as_mut_ptr(), 128).max(0) as usize;
                let (p, e) = template_preview(&String::from_utf16_lossy(&b[..n]));
                crate::settings_view::set_template_feedback(&p, &e, !e.is_empty());
            }
            crate::settings_view::set_pause_label(&crate::shortcuts::label(pause_packed));
            let exportbutton = button(hwnd, "Export settings", 67, 306, 900, 120, font);
            let importbutton = button(hwnd, "Import settings", 68, 306, 900, 120, font);
            settings_controls.extend([exportbutton, importbutton]);
            ShowWindow(exportbutton, SW_HIDE);
            ShowWindow(importbutton, SW_HIDE);
            // Only present while an earlier import is locked; resolving keeps the evidence file.
            let resolvebutton = button(hwnd, "Turn imports back on", 69, 306, 900, 256, font);
            ShowWindow(resolvebutton, SW_HIDE);
            if startup_blocked {
                settings_controls.push(resolvebutton);
                EnableWindow(importbutton, 0);
            } else {
                DestroyWindow(resolvebutton);
            }
            let mut import_pending = false;
            let mut import_resolved = false;
            let say = |m: &str| {
                set(result, m);
                crate::settings_view::set_backup_status(m);
            };
            let setupbutton = button(hwnd, "Show setup guide next launch", 65, 48, 850, 256, font);
            settings_controls.push(setupbutton);
            ShowWindow(setupbutton, SW_HIDE);
            let commsbutton = button(hwnd, "Voice chat apps...", 73, 306, 650, 256, font);

            settings_controls.push(commsbutton);
            ShowWindow(commsbutton, SW_HIDE);
            let devicebutton = button(hwnd, "Capture devices...", 79, 48, 700, 256, font);
            let folderbutton = button(hwnd, "Video save folder...", 80, 306, 700, 256, font);
            settings_controls.extend([devicebutton, folderbutton]);
            ShowWindow(devicebutton, SW_HIDE);
            ShowWindow(folderbutton, SW_HIDE);
            let quicksystem = button(hwnd, "System audio ON", 86, 48, 466, 248, font);
            let quickmic = button(hwnd, "Mic OFF", 87, 310, 466, 252, font);
            crate::ui::tooltip(
                quicksystem,
                "Turn system capture on/off. Clears the unsaved replay buffer.",
            );
            crate::ui::tooltip(
                quickmic,
                "Turn microphone capture on/off. Clears the unsaved replay buffer.",
            );
            let mut settings_open = false;
            sync_len(lenslider, durationbutton, &lenquick, buffer_seconds);
            let keys = [(1, 0x47), (2, 0x4f), (3, 0x51)];
            let mut save_shortcut = "Alt+G".to_string();
            let mut save_modifiers = MOD_ALT;
            let mut save_key = 0x47u32;
            let mut active_packed = 0x447u32;
            let mut save_registered =
                RegisterHotKey(std::ptr::null_mut(), 1, MOD_ALT | MOD_NOREPEAT, 0x47) != 0;
            let mut shortcut_warning = String::new();
            if !save_registered {
                let code = GetLastError();
                writeln!(log.lock().unwrap(), "HOTKEY Alt+G unavailable code={code}").ok();
                save_modifiers = MOD_ALT | MOD_CONTROL;
                save_registered =
                    RegisterHotKey(std::ptr::null_mut(), 1, save_modifiers | MOD_NOREPEAT, 0x47)
                        != 0;
                active_packed = 0x647;
                save_shortcut = if save_registered {
                    "Ctrl+Alt+G"
                } else {
                    "button only"
                }
                .to_string();
                shortcut_warning = format!("Alt+G is owned by another app. Save: {save_shortcut}");
            }
            for (id, key) in [(2, 0x4f), (3, 0x51)] {
                if RegisterHotKey(std::ptr::null_mut(), id, MOD_ALT | MOD_NOREPEAT, key) == 0 {
                    writeln!(
                        log.lock().unwrap(),
                        "HOTKEY registration failed id={id} code={}",
                        GetLastError()
                    )
                    .ok();
                }
            }
            // The pause shortcut (Alt+P unless changed) pauses or resumes capture, same guarded path as the Home button.
            let (pause_mods, pause_vk) = crate::shortcuts::split(pause_packed);
            let mut pause_registered =
                RegisterHotKey(std::ptr::null_mut(), 94, pause_mods | MOD_NOREPEAT, pause_vk) != 0;
            if !pause_registered {
                let pause_name = crate::shortcuts::label(pause_packed);
                writeln!(log.lock().unwrap(), "HOTKEY {pause_name} (pause) unavailable code={}", GetLastError()).ok();
                if shortcut_warning.is_empty() {
                    shortcut_warning = format!("{pause_name} (pause) is owned by another app.");
                }
            }
            set(
                savebutton,
                &format!("Save last {buffer_seconds}s     {save_shortcut}"),
            );
            let shortcutlabel = label(
                hwnd,
                &format!("Save: {save_shortcut} | Clip contents stay on this device."),
                26,
                666,
                902,
                23,
                font,
            );
            if !shortcut_warning.is_empty() {
                set(shortcutlabel, &shortcut_warning);
            }
            for (key, h) in [
                ("status", status),
                ("viewheading", viewheading),
                ("heading", appheading),
                ("simplehint", simplehint),
                ("warning", warning),
                ("privacy", privacy),
                ("details", detail),
                ("audio", audiolabel),
                ("rightheading", rightheading),
                ("righthint", righthint),
                ("footer", shortcutlabel),
                ("result", result),
                ("open", openclipsbutton),
                ("select", selectclips),
                ("delete", deleteclips),
                ("filter", filterclips),
                ("tagline", tagline),
                ("buffer", bufferbar),
                ("sources", sourceslabel),
                ("source-note", sourcenote),
                ("encoder", encoderlabel),
            ] {
                SetPropW(hwnd, wide(&format!("slot-{key}")).as_ptr(), h as _);
            }
            crate::catalog::filter().lock().unwrap().oldest =
                fs::read_to_string(root.join("library-sort.txt"))
                    .is_ok_and(|s| s.trim() == "oldest");
            {
                let mut f = crate::catalog::filter().lock().unwrap();
                f.sort = fs::read_to_string(root.join("library-sort.txt"))
                    .ok()
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(if f.oldest { 1 } else { 0 });
                f.sort = f.sort.min(3);
                SendMessageW(GetDlgItem(hwnd, 77), CB_SETCURSEL, f.sort, 0);
            }
            refresh_recent(&dir, &recentrows, &mut recentpaths, true);
            crate::settings_view::init(hwnd, &settings_controls, font);
            if startup_blocked {
                crate::settings_view::set_import_state(2);
            }
            let mut lock_note: Option<String> = startup_note.clone();
            SetPropW(sourcenote, wide("replay-note").as_ptr(), 1usize as _);
            SetPropW(status, wide("replay-status").as_ptr(), 1usize as _);
            crate::replay_design::prepare(hwnd, font);
            let display_items = crate::display::snapshot(hwnd);
            crate::display::apply(
                hwnd,
                &display_items,
                layout,
                false,
                &recentrows,
                large_cards,
            );
            let mut hotkey_retry = Instant::now();
            let mut pause_retry = Instant::now();
            crate::startup_note("Main window ready");
            let tray_ok = crate::background::install(hwnd);
            SetPropW(hwnd, wide("tray-key").as_ptr(), active_packed as usize as _);
            if !std::env::args().any(|x| x == "--background") || !tray_ok {
                ShowWindow(hwnd, SW_SHOW);
            }
            if !tray_ok {
                set(
                    result,
                    "Tray icon unavailable. X will exit instead of hiding.",
                );
            }
            UpdateWindow(hwnd);
            let loaded = crate::capture_settings::CaptureSettings::load(&root);
            let gain = Arc::new(AtomicI32::new(loaded.system_gain));
            let mut audio_on = loaded.system_audio;
            let mut mic_on = loaded.microphone;
            let mic_gain = Arc::new(AtomicI32::new(loaded.microphone_gain));
            let mut mic_status = Arc::new(Mutex::new(
                if mic_on { "Starting mic..." } else { "Mic OFF" }.to_string(),
            ));
            let mut capture_width = loaded.width;
            let mut capture_fps = loaded.fps;
            let mut audio_status = Arc::new(Mutex::new(String::from("Starting system audio...")));
            let index = loaded.monitor;
            let mut compat = loaded.compatibility;
            let mut desired_compat = compat;
            let mut saved_capture = loaded;
            SendMessageW(pathcombo, CB_SETCURSEL, if compat { 0 } else { 1 }, 0);
            SendMessageW(
                rescombo,
                CB_SETCURSEL,
                match capture_width {
                    854 => 0,
                    1920 => 2,
                    _ => 1,
                },
                0,
            );
            SendMessageW(
                fpscombo,
                CB_SETCURSEL,
                if capture_fps == 60 { 1 } else { 0 },
                0,
            );
            SendMessageW(sysslider, 0x405, 1, gain.load(Ordering::Relaxed) as isize);
            SendMessageW(
                micslider,
                0x405,
                1,
                mic_gain.load(Ordering::Relaxed) as isize,
            );
            set(sysvalue, &format!("+{} dB", gain.load(Ordering::Relaxed)));
            set(
                micvalue,
                &format!("+{} dB", mic_gain.load(Ordering::Relaxed)),
            );
            set(
                audiobutton,
                if audio_on {
                    "System audio ON"
                } else {
                    "System audio OFF"
                },
            );
            set(micbutton, if mic_on { "Mic ON" } else { "Mic OFF" });
            let mut ring = Arc::new(Mutex::new(Ring::with_seconds(buffer_seconds)));
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if job.is_null()
                || SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
            {
                return Err("Cannot safely manage capture process.".into());
            }
            set(
                pathbutton,
                if compat {
                    "CPU compatibility"
                } else {
                    "GPU conversion"
                },
            );
            let recordtap = crate::recording::new();
            let mut child = start(
                &exe,
                index,
                compat,
                audio_status.clone(),
                gain.clone(),
                log.clone(),
                audio_on,
                capture_width,
                capture_fps,
                mic_on,
                mic_gain.clone(),
                mic_status.clone(),
            )
            .map_err(|e| e.to_string())?;
            if AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
                let _ = child.kill();
                return Err("Cannot manage capture child.".into());
            }
            attach_reader(&mut child, ring.clone(), log.clone(), recordtap.clone());
            let (tx, rx) = mpsc::channel();
            let saving = Arc::new(AtomicBool::new(false));
            let mut last_audio_status = String::new();
            let mut recording = false;
            let mut recordfinishing = false;
            let mut recovering = false;
            let mut recordbegan = Instant::now();
            let (recordtx, recordrx) = mpsc::channel();
            let mut paused = false;
            let mut ready = false;
            let mut failed = false;
            let mut began = Instant::now();
            let mut last = Instant::now();
            if pref("hotkey", 0x447) != 0x447 {
                PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 32, 0);
            }
            let mut msg: MSG = std::mem::zeroed();
            loop {
                let current_capture = crate::capture_settings::CaptureSettings {
                    monitor: index,
                    compatibility: desired_compat,
                    width: capture_width,
                    fps: capture_fps,
                    system_audio: audio_on,
                    microphone: mic_on,
                    system_gain: gain.load(Ordering::Relaxed),
                    microphone_gain: mic_gain.load(Ordering::Relaxed),
                };
                if !import_pending && current_capture != saved_capture {
                    if let Err(e) = current_capture.save(&root) {
                        set(
                            result,
                            "Capture settings could not be saved. See Diagnostics.",
                        );
                        writeln!(log.lock().unwrap(), "PREFERENCES save failed: {e}").ok();
                    } else {
                        writeln!(log.lock().unwrap(), "PREFERENCES saved").ok();
                    }
                    saved_capture = current_capture;
                }
                let (selecting, nsel) = {
                    let s = crate::library::state().lock().unwrap();
                    (s.active, s.paths.len())
                };
                let stamp = if selecting { nsel + 1 } else { 0 };
                if GetPropW(hwnd, wide("selection-stamp").as_ptr()) as usize != stamp {
                    SetPropW(hwnd, wide("selection-stamp").as_ptr(), stamp as _);
                    set(selectclips, if selecting { "Done" } else { "Select clips" });
                    set(deleteclips, &format!("Delete selected ({nsel})"));
                    EnableWindow(deleteclips, if nsel > 0 { 1 } else { 0 });
                    for row in &recentrows {
                        InvalidateRect(*row, std::ptr::null(), 0);
                    }
                    crate::display::apply(
                        hwnd,
                        &display_items,
                        layout,
                        settings_open,
                        &recentrows,
                        large_cards,
                    );
                }
                SetPropW(hwnd, wide("tray-paused").as_ptr(), paused as usize as _);
                SetPropW(
                    hwnd,
                    wide("tray-recording").as_ptr(),
                    (recording || recordfinishing) as usize as _,
                );
                set(
                    quicksystem,
                    if audio_on { "System ON" } else { "System OFF" },
                );
                set(quickmic, if mic_on { "Mic ON" } else { "Mic OFF" });
                if !settings_open {
                    crate::replay_design::source_state(
                        audiobutton,
                        audio_on,
                        &format!(
                            "{} / +{} dB",
                            if audio_on {
                                "System enabled"
                            } else {
                                "Off, not in new clips"
                            },
                            gain.load(Ordering::Relaxed)
                        ),
                    );
                    crate::replay_design::source_state(
                        micbutton,
                        mic_on,
                        &format!(
                            "{} / +{} dB",
                            if mic_on {
                                "Microphone enabled"
                            } else {
                                "Off, click to enable"
                            },
                            mic_gain.load(Ordering::Relaxed)
                        ),
                    );
                } else {
                    set(
                        audiobutton,
                        if audio_on {
                            "System audio ON"
                        } else {
                            "System audio OFF"
                        },
                    );
                    set(
                        micbutton,
                        if mic_on {
                            "Microphone ON"
                        } else {
                            "Microphone OFF"
                        },
                    );
                }
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_KEYDOWN && msg.wParam == 0x0d && GetDlgCtrlID(msg.hwnd) == crate::ctl_ids::TEMPLATE_EDIT as i32 {
                        PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, crate::ctl_ids::APPLY_TEMPLATE, 0);
                        continue;
                    }
                    if msg.message == WM_KEYDOWN && msg.wParam == 0x0d && (GetDlgCtrlID(msg.hwnd) == 95 || GetDlgCtrlID(msg.hwnd) == crate::ctl_ids::TAGLINE_EDIT as i32) {
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(hwnd);
                        continue;
                    }
                    if msg.message == WM_HOTKEY {
                        if (recording || recordfinishing)
                            && [3, 4, 7, 8, 9, 10, 12, 20, 21, 23, 30, 42, 46, 94].contains(&msg.wParam)
                        {
                            crate::background::reveal(hwnd);
                            set(result,"Stop recording and wait for it to finish before quitting or changing capture.");
                            continue;
                        }
                        SetPropW(hwnd, wide("tray-paused").as_ptr(), paused as usize as _);
                        SetPropW(
                            hwnd,
                            wide("tray-recording").as_ptr(),
                            (recording || recordfinishing) as usize as _,
                        );
                        if (recording || recordfinishing) && msg.wParam == 3 {
                            crate::background::reveal(hwnd);
                        }
                        if msg.wParam == 86 {
                            msg.wParam = 7;
                        }
                        if msg.wParam == 87 {
                            msg.wParam = 12;
                        }
                        if msg.wParam == 94 {
                            // Alt+P: the same guarded pause/resume as the Home button.
                            msg.wParam = 10;
                        }
                        if import_pending
                            && (msg.wParam == crate::ctl_ids::REPLAY_SLIDER || crate::ctl_ids::QUICK_LEN.contains(&msg.wParam) || matches!(msg.wParam, 6 | 7 | 8 | 9 | 12 | 13 | 20 | 21 | 22 | 23 | 26 | 27 | 32 | 33 | 34 | 35 | 42 | 47 | 48 | 62 | 63 | 64 | 65 | 66 | 73 | 79 | 80 | 97 | crate::ctl_ids::APPLY_PAUSE | crate::ctl_ids::APPLY_TEMPLATE))
                        {
                            set(result, "Settings are locked after an import. Reopen ZimClip first.");
                            continue;
                        }
                        match msg.wParam {
                            89 => {
                                // Interface style switch. No capture, playback or
                                // media state is touched; only GDI colours and paths.
                                let polished = !crate::polished::enabled();
                                crate::polished::set(polished);
                                // Never open a switched style straight onto the Clips page.
                                crate::polished::set_nav_page(0);
                                set(
                                    modebutton,
                                    if polished { "Legacy UI" } else { "Polished UI" },
                                );
                                // Keep the user's current theme/accent; only the
                                // drawing tokens change with the interface mode.
                                let theme_index = crate::ui::theme_index();
                                crate::ui::set_theme(theme_index);
                                crate::ui::set_accent(crate::ui::accent_index());
                                if let Err(e) = crate::polished::save_prefs(
                                    &root,
                                    theme_index,
                                    active_packed as u32,
                                    sound_on,
                                    toast_on,
                                ) {
 set(result, &format!("Could not save settings: {e}. The change applies for now only."));
 }
                                let in_settings = !GetPropW(
                                    hwnd,
                                    wide("display-settings").as_ptr(),
                                )
                                .is_null();
                                crate::display::apply(
                                    hwnd,
                                    &display_items,
                                    layout,
                                    in_settings,
                                    &recentrows,
                                    large_cards,
                                );
                                density_labels(largebutton, sizebutton, large_cards);
                                refresh_recent(&dir, &recentrows, &mut recentpaths, true);
                                InvalidateRect(hwnd, std::ptr::null(), 0);
                                EnumChildWindows(hwnd, Some(crate::ui::invalidate_child), 0);
                                set(
                                    result,
                                    if polished {
                                        "Reference interface on. Use Legacy UI for the old, lighter view."
                                    } else {
                                        "Legacy interface on. Use Polished UI to return to the reference design."
                                    },
                                );
                            }
                            91 => {
                                let reduced = !crate::ui::motion::reduced();
                                crate::ui::motion::set_reduced(reduced);
                                set(
                                    motionbutton,
                                    if reduced { "Motion: reduced" } else { "Motion: full" },
                                );
                                if let Err(e) = crate::polished::save_prefs(
                                    &root,
                                    crate::ui::theme_index(),
                                    active_packed as u32,
                                    sound_on,
                                    toast_on,
                                ) {
 set(result, &format!("Could not save settings: {e}. The change applies for now only."));
 }
                                InvalidateRect(motionbutton, std::ptr::null(), 0);
                            }
                            82..=84 => {
                                crate::settings_view::jump(msg.wParam - 82);
                            }
                            61 => {
                                crate::background::reveal(hwnd);
                            }
                            69 => {
                                if crate::editor::confirm_themed(hwnd, font, "Turn imports back on?\nAn earlier import did not finish. This step does not change any settings. They may be partly changed, a mix of old and new values. The unfinished record is kept as a file next to ZimClip, not deleted.", "Turn on") {
                                    match crate::settings_bundle::resolve_lock(&root) {
                                        Ok(m) => {
                                            say(&m);
                                            import_resolved = true;
                                            lock_note = None;
                                            crate::settings_view::set_import_state(0);
                                            EnableWindow(importbutton, 1);
                                            ShowWindow(resolvebutton, SW_HIDE);
                                            EnableWindow(resolvebutton, 0);
                                        }
                                        Err(e) => say(&e),
                                    }
                                }
                            }
                            67 => {
                                if let Some(path) = crate::settings_bundle_ui::pick_save(hwnd) {
                                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                    match fs::write(&path, crate::settings_bundle::export(&root)) {
                                        Ok(()) => say(&format!("Saved {name}. Partial backup only: custom colours, fonts, folders, devices and shortcuts are not in it.")),
                                        Err(e) => say(&format!("Could not save the file: {e}")),
                                    }
                                }
                            }
                            68 => {
                                if saving.load(Ordering::SeqCst) || crate::editor::is_saving() {
                                    say("Wait for the current save before importing settings.");
                                    continue;
                                }
                                if let Some(path) = crate::settings_bundle_ui::pick_open(hwnd) {
                                    let read = match fs::metadata(&path) {
                                        Ok(m) if m.len() as usize > crate::settings_bundle::MAX_BYTES => Err("Settings file is too large.".to_string()),
                                        Ok(_) => fs::read_to_string(&path).map_err(|_| "That file could not be read as text.".to_string()),
                                        Err(e) => Err(e.to_string()),
                                    };
                                    match read.and_then(|t| crate::settings_bundle::parse(&t)) {
                                        Err(e) => say(&format!("Nothing was imported. {e}")),
                                        Ok(b) => {
                                            let plan = crate::settings_bundle::plan(&root, &b);
                                            if plan.changes.is_empty() {
                                                say("Nothing to import: these settings already match.");
                                            } else {
                                                // Every changed value is listed (at most 20 lines).
                                                let lines: Vec<String> = plan.preview.clone();
                                                let text = format!(
                                                    "Import these settings?\n{}\x0b\x0b{}Only the listed items change. Custom colours, fonts, folders, devices, shortcuts and the custom file-name template are not touched. Appearance and layout show after you quit and reopen ZimClip.",
                                                    lines.join("\x0b"),
                                                    if plan.capture_changed { "Capture values are used after you quit and reopen ZimClip. The running recording is not restarted.\x0b" } else { "" }
                                                );
                                                if crate::editor::confirm_themed(hwnd, font, &text, "Import") {
                                                    match crate::settings_bundle::apply(&root, &plan.changes) {
                                                        Ok(()) => {
                                                            // Files on disk are now the truth. Lock every settings control and stop
                                                            // the autosave/exit save from writing old in-memory values over them.
                                                            import_pending = true;
                                                            for c in &settings_controls {
                                                                if *c != exportbutton {
                                                                    EnableWindow(*c, 0);
                                                                }
                                                            }
                                                            set(importbutton, "Import locked");
                                                            crate::settings_view::show_import_lock();
                                                            say("Settings imported and saved. Settings in this window are locked and may show old values until ZimClip is reopened.");
                                                            let busy = recording || recordfinishing || recovering || saving.load(Ordering::SeqCst) || crate::editor::is_saving();
                                                            if busy {
                                                                say("Settings imported and saved, but a recording or save is in progress, so ZimClip was not asked to quit. Finish it, then close and reopen ZimClip. Settings here stay locked.");
                                                            } else if crate::editor::confirm_themed(hwnd, font, "Settings imported.\nQuit ZimClip now to use them?\nQuitting stops recording and clears the unsaved replay. Saved clips are not affected. Choose Cancel to keep recording; settings stay locked until you reopen ZimClip.", "Quit now") {
                                                                PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 3, 0);
                                                            }
                                                        }
                                                        Err(e) => say(&e),
                                                    }
                                                } else {
                                                    say("Import cancelled. Nothing changed.");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            97 => {
                                if let Some(dir) = crate::file_prefs::prefs_file().parent().map(|d| d.to_path_buf()) {
                                    let cur = crate::file_prefs::name_style(&dir);
                                    let next = if cur >= crate::file_prefs::NAME_STYLES { 0 } else { (cur + 1) % crate::file_prefs::NAME_STYLES };
                                    match crate::file_prefs::set_name_style(&dir, next) {
                                        Ok(()) => {
                                            set(namebutton, crate::file_prefs::name_label(next));
                                            crate::settings_view::set_name_example(&name_example(&dir));
                                            set(result, "New clips use this name style. Existing clips are not renamed.");
                                            if crate::file_prefs::template(&dir).is_some() {
                                                crate::settings_view::set_template_feedback(&template_preview_of(tpledit).0, "Preset chosen. Your custom template is kept; press Apply template to use it again.", false);
                                            }
                                        }
                                        Err(e) => set(result, &format!("Could not save the setting: {e}")),
                                    }
                                }
                            }
                            66 => {
                                if let Some(dir) = crate::file_prefs::prefs_file().parent().map(|d| d.to_path_buf()) {
                                    let next = !crate::file_prefs::close_hides_to_tray(&dir);
                                    match crate::file_prefs::set_close_hides_to_tray(&dir, next) {
                                        Ok(()) => {
                                            close_hides = next;
                                            set(closebutton, crate::file_prefs::close_label(close_hides));
                                            set(
                                                result,
                                                if close_hides {
                                                    "The close button now hides ZimClip to the tray. Recording continues."
                                                } else {
                                                    "The close button now exits ZimClip, which stops recording."
                                                },
                                            );
                                        }
                                        Err(e) => set(result, &format!("Could not save the setting: {e}")),
                                    }
                                }
                            }
                            65 => {
                                // Only the "setup finished" marker is removed. No setting, clip or
                                // preference changes now; the guide itself saves on its Done.
                                match std::fs::remove_file(root.join("welcome-complete.txt")) {
                                    Ok(()) => set(result, "The setup guide will open the next time ZimClip starts. Nothing else changed."),
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                        set(result, "The setup guide is already set to open on the next start.")
                                    }
                                    Err(e) => set(result, &format!("Could not reset the setup guide: {e}")),
                                }
                            }
                            64 => {
                                let dir = crate::file_prefs::prefs_file().parent().map(|d| d.to_path_buf());
                                if let Some(dir) = dir {
                                    let next = crate::file_prefs::next_share_default(crate::file_prefs::share_default_mb(&dir));
                                    match crate::file_prefs::set_share_default_mb(&dir, next) {
                                        Ok(()) => {
                                            set(sharebutton, &crate::file_prefs::share_label(Some(&dir)));
                                            set(result, "Share size default saved. It only preselects the size in the clip editor.");
                                        }
                                        Err(e) => set(result, &format!("Could not save the setting: {e}")),
                                    }
                                }
                            }
                            63 => {
                                // Flip the real stored state, which the editor's checkbox may have changed.
                                confirm_deletes = !crate::file_prefs::delete_confirm_skipped(&crate::file_prefs::prefs_file());
                                match crate::file_prefs::set_delete_confirm_skipped(&crate::file_prefs::prefs_file(), confirm_deletes) {
                                    Ok(()) => {
                                        confirm_deletes = !confirm_deletes;
                                        set(
                                            confirmbutton,
                                            if confirm_deletes { "Confirm before deleting ON" } else { "Confirm before deleting OFF" },
                                        );
                                        set(
                                            result,
                                            if confirm_deletes {
                                                "Deleting an open clip will ask first."
                                            } else {
                                                "Deleting an open clip will no longer ask. Clips still go to Trash."
                                            },
                                        );
                                    }
                                    Err(e) => set(result, &format!("Could not save the setting: {e}")),
                                }
                            }
                            62 => match crate::background::startup(!auto_start) {
                                Ok(()) => {
                                    auto_start = !auto_start;
                                    set(
                                        startupbutton,
                                        if auto_start {
                                            "Start with Windows ON"
                                        } else {
                                            "Start with Windows OFF"
                                        },
                                    );
                                    set(
                                        result,
                                        if auto_start {
                                            "Starts hidden at your next Windows sign-in. Keep this folder in place."
                                        } else {
                                            "Start with Windows is off."
                                        },
                                    );
                                }
                                Err(e) => set(result, &e),
                            },
                            1 => {
                                writeln!(log.lock().unwrap(), "SAVE requested via shortcut/button")
                                    .ok();
                                if failed || !ready {
                                    set(
                                        result,
                                        "Capture is not ready. Wait or press Restart capture.",
                                    );
                                    continue;
                                }
                                if saving.swap(true, Ordering::SeqCst) {
                                    set(result, "Still saving the previous clip.");
                                    continue;
                                }
                                let snapshot = {
                                    let r = ring.lock().unwrap();
                                    if r.parts
                                        .back()
                                        .is_some_and(|p| p.at.elapsed() > Duration::from_secs(5))
                                    {
                                        None
                                    } else {
                                        r.snapshot()
                                    }
                                };
                                if let Some(data) = snapshot {
                                    set(result, "Saving replay...");
                                    let e = exe.clone();
                                    let d = dir.clone();
                                    let t = tx.clone();
                                    let s = saving.clone();
                                    thread::spawn(move || {
                                        let _ = t.send(save(&e, &d, data));
                                        s.store(false, Ordering::SeqCst);
                                    });
                                } else {
                                    saving.store(false, Ordering::SeqCst);
                                    set(result, "No recent frames. Press Restart capture.");
                                }
                            }
            92 => {
                                let prev_theme = crate::ui::theme_index();
                                let prev_accent = crate::ui::accent_index();
                                let prev_custom = crate::custom_theme::current();
                                let start = crate::custom_dialog::Edit {
                                    custom: prev_custom,
                                    accent: prev_accent,
                                    presets: crate::custom_theme::load_presets(&root),
                                };
                                let preview = |c: crate::custom_theme::Custom, a: usize| {
                                    crate::custom_theme::set_current(c);
                                    crate::ui::set_theme(crate::custom_theme::THEME_INDEX);
                                    crate::ui::set_accent(a);
                                    InvalidateRect(hwnd, std::ptr::null(), 0);
                                    EnumChildWindows(hwnd, Some(crate::ui::invalidate_child), 0);
                                };
                                let mut problems: Vec<String> = Vec::new();
                                match crate::custom_dialog::run(hwnd, font, start, &preview) {
                                    crate::custom_dialog::Outcome::Apply(e) => {
                                        crate::custom_theme::set_current(e.custom);
                                        crate::ui::set_theme(crate::custom_theme::THEME_INDEX);
                                        crate::ui::set_accent(e.accent);
                                        SendMessageW(themecombo, CB_SETCURSEL, crate::custom_theme::THEME_INDEX, 0);
                                        if let Err(x) = crate::custom_theme::save_custom(&root, e.custom) {
                                            problems.push(x);
                                        }
                                        if let Err(x) = crate::custom_theme::save_presets(&root, &e.presets) {
                                            problems.push(x);
                                        }
                                        if let Err(x) = fs::write(root.join("appearance.ini"), e.accent.to_string()) {
                                            problems.push(x.to_string());
                                        }
                                        match crate::custom_theme::warning(e.custom) {
                                            Some(w) => set(result, &w),
                                            None => set(result, "Custom theme applied."),
                                        }
                                    }
                                    crate::custom_dialog::Outcome::Reset(p) => {
                                        let back = if prev_theme < 6 { prev_theme } else { 0 };
                                        crate::ui::set_theme(back);
                                        crate::ui::set_accent(prev_accent);
                                        SendMessageW(themecombo, CB_SETCURSEL, back, 0);
                                        if let Err(x) = crate::custom_theme::save_presets(&root, &p) {
                                            problems.push(x);
                                        }
                                        set(result, "Back to a built-in theme. Your saved presets are kept.");
                                    }
                                    crate::custom_dialog::Outcome::Cancel => {
                                        crate::custom_theme::set_current(prev_custom);
                                        crate::ui::set_theme(prev_theme);
                                        crate::ui::set_accent(prev_accent);
                                        SendMessageW(themecombo, CB_SETCURSEL, prev_theme, 0);
                                    }
                                }
                                if let Err(x) = crate::polished::save_prefs(
                                    &root,
                                    SendMessageW(themecombo, CB_GETCURSEL, 0, 0).max(0) as usize,
                                    active_packed as u32,
                                    sound_on,
                                    toast_on,
                                ) {
                                    problems.push(x);
                                }
                                if !problems.is_empty() {
                                    set(
                                        result,
                                        &format!("Could not save: {}. The look applies for now only; the earlier saved look is kept.", problems.join("; ")),
                                    );
                                }
                                InvalidateRect(hwnd, std::ptr::null(), 0);
                                EnumChildWindows(hwnd, Some(crate::ui::invalidate_child), 0);
                            }
                            31 | 34 | 35 => {
                                if msg.wParam == 31 {
                                    let picked = SendMessageW(themecombo, CB_GETCURSEL, 0, 0).max(0) as usize;
                                    crate::ui::set_theme(picked);
                                    if picked == crate::custom_theme::THEME_INDEX {
                                        // Choosing Custom opens the colour editor right away.
                                        PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 92, 0);
                                    }
                                }
                                if msg.wParam == 34 {
                                    sound_on = !sound_on;
                                    set(
                                        soundbutton,
                                        if sound_on {
                                            "Save ding ON"
                                        } else {
                                            "Save ding OFF"
                                        },
                                    );
                                }
                                if msg.wParam == 35 {
                                    toast_on = !toast_on;
                                    set(
                                        toastbutton,
                                        if toast_on {
                                            "Clip notification ON"
                                        } else {
                                            "Clip notification OFF"
                                        },
                                    );
                                }
                                if let Err(e) = crate::polished::save_prefs(
                                    &root,
                                    SendMessageW(themecombo, CB_GETCURSEL, 0, 0).max(0) as usize,
                                    active_packed as u32,
                                    sound_on,
                                    toast_on,
                                ) {
 set(result, &format!("Could not save settings: {e}. The change applies for now only."));
 }
                                InvalidateRect(hwnd, std::ptr::null(), 0);
                                EnumChildWindows(hwnd, Some(crate::ui::invalidate_child), 0);
                                let editor =
                                    FindWindowW(wide("ZimClipEditor").as_ptr(), std::ptr::null());
                                if !editor.is_null() {
                                    crate::ui::apply_identity(editor, true);
                                    InvalidateRect(editor, std::ptr::null(), 0);
                                    EnumChildWindows(editor, Some(crate::ui::invalidate_child), 0);
                                }
                            }
                            47 => {
                                set(result,"Editing shortcut. Global save and pause keys are paused until you leave this field.");
                                if save_registered {
                                    UnregisterHotKey(std::ptr::null_mut(), 1);
                                    save_registered = false;
                                }
                                if pause_registered {
                                    UnregisterHotKey(std::ptr::null_mut(), 94);
                                    pause_registered = false;
                                }
                            }
                            48 => {
                                save_registered = RegisterHotKey(
                                    std::ptr::null_mut(),
                                    1,
                                    save_modifiers | MOD_NOREPEAT,
                                    save_key,
                                ) != 0;
                                let (pm, pk) = crate::shortcuts::split(pause_packed);
                                pause_registered =
                                    RegisterHotKey(std::ptr::null_mut(), 94, pm | MOD_NOREPEAT, pk) != 0;
                            }
                            32 => {
                                let packed = SendMessageW(hotkey, 0x402, 0, 0) as u32;
                                let key = packed & 255;
                                let flags = (packed >> 8) & 255;
                                let modifiers = (if flags & 1 != 0 { MOD_SHIFT } else { 0 })
                                    | (if flags & 2 != 0 { MOD_CONTROL } else { 0 })
                                    | (if flags & 4 != 0 { MOD_ALT } else { 0 });
                                if let Err(e) = crate::shortcuts::validate(crate::shortcuts::normalize(packed), pause_packed, "Pause") {
                                    set(result, &e);
                                    crate::settings_view::set_shortcut_msg(0, &format!("{e} Save shortcut unchanged."), true);
                                    SendMessageW(hotkey, 0x401, active_packed as usize, 0);
                                    continue;
                                }
                                if key == save_key && modifiers == save_modifiers && save_registered
                                {
                                    set(result, "That shortcut is already active.");
                                    crate::settings_view::set_shortcut_msg(0, "That shortcut is already active.", false);
                                    continue;
                                }
                                if RegisterHotKey(
                                    std::ptr::null_mut(),
                                    99,
                                    modifiers | MOD_NOREPEAT,
                                    key,
                                ) == 0
                                {
                                    set(result,"That shortcut is busy. Your current shortcut is unchanged.");
                                    crate::settings_view::set_shortcut_msg(0, "That shortcut is busy. Your current shortcut is unchanged.", true);
                                    SendMessageW(hotkey, 0x401, active_packed as usize, 0);
                                    continue;
                                }
                                UnregisterHotKey(std::ptr::null_mut(), 99);
                                UnregisterHotKey(std::ptr::null_mut(), 1);
                                save_registered = RegisterHotKey(
                                    std::ptr::null_mut(),
                                    1,
                                    modifiers | MOD_NOREPEAT,
                                    key,
                                ) != 0;
                                if save_registered {
                                    save_modifiers = modifiers;
                                    save_key = key;
                                    active_packed = packed;
                                    SetPropW(hwnd, wide("tray-key").as_ptr(), packed as usize as _);
                                    save_shortcut = crate::shortcuts::label(packed);
                                    writeln!(
                                        log.lock().unwrap(),
                                        "HOTKEY applied {} packed={packed}",
                                        save_shortcut
                                    )
                                    .ok();

                                    set(
                                        savebutton,
                                        &format!("Save last {buffer_seconds}s   {save_shortcut}"),
                                    );
                                    set(shortcutlabel,&format!("Save: {save_shortcut} | Clip contents stay on this device."));
                                    set(result, "Save shortcut applied.");
                                    crate::settings_view::set_shortcut_msg(0, "Save shortcut applied.", false);
                                    if let Err(e) = crate::polished::save_prefs(
                                        &root,
                                        SendMessageW(themecombo, CB_GETCURSEL, 0, 0).max(0) as usize,
                                        packed,
                                        sound_on,
                                        toast_on,
                                    ) {
 set(result, &format!("Could not save settings: {e}. The change applies for now only."));
 }
                                } else {
                                    save_registered = RegisterHotKey(
                                        std::ptr::null_mut(),
                                        1,
                                        save_modifiers | MOD_NOREPEAT,
                                        save_key,
                                    ) != 0;
                                    SendMessageW(hotkey, 0x401, active_packed as usize, 0);
                                    let why = if save_registered {
                                        "Could not apply. Previous shortcut restored."
                                    } else {
                                        "Shortcut unavailable. Save button still works."
                                    };
                                    set(result, why);
                                    crate::settings_view::set_shortcut_msg(0, why, true);
                                }
                                // Focus-safe: while a shortcut box has focus the global keys stay off
                                // (focus loss re-registers them), so typing a key never fires an action.
                                if GetFocus() == hotkey || GetFocus() == pausekey {
                                    if save_registered {
                                        UnregisterHotKey(std::ptr::null_mut(), 1);
                                        save_registered = false;
                                    }
                                    if pause_registered {
                                        UnregisterHotKey(std::ptr::null_mut(), 94);
                                        pause_registered = false;
                                    }
                                }
                            }
                            crate::ctl_ids::APPLY_PAUSE => {
                                let packed = crate::shortcuts::normalize(SendMessageW(pausekey, 0x402, 0, 0) as u32);
                                let say = |m: &str, bad: bool| crate::settings_view::set_shortcut_msg(1, m, bad);
                                if let Err(e) = crate::shortcuts::validate(packed, active_packed, "Save") {
                                    say(&format!("{e} Pause shortcut unchanged."), true);
                                    SendMessageW(pausekey, 0x401, pause_packed as usize, 0);
                                    continue;
                                }
                                if packed == pause_packed && pause_registered {
                                    say("That shortcut is already active.", false);
                                    continue;
                                }
                                let (m, k) = crate::shortcuts::split(packed);
                                // 1. Is the key free? Test it on a scratch id; nothing else changes yet.
                                if RegisterHotKey(std::ptr::null_mut(), 99, m | MOD_NOREPEAT, k) == 0 {
                                    say("That shortcut is busy. Your current pause shortcut is unchanged.", true);
                                    SendMessageW(pausekey, 0x401, pause_packed as usize, 0);
                                    continue;
                                }
                                UnregisterHotKey(std::ptr::null_mut(), 99);
                                // 2. Save the choice. If this fails nothing was swapped.
                                let dir = crate::file_prefs::prefs_file().parent().map(|d| d.to_path_buf()).unwrap_or_default();
                                if let Err(e) = crate::file_prefs::set_pause_key(&dir, packed) {
                                    say(&format!("Could not save the pause shortcut: {e}. Your current one is unchanged."), true);
                                    SendMessageW(pausekey, 0x401, pause_packed as usize, 0);
                                    continue;
                                }
                                // 3. Swap the live key. If it fails, put the saved choice and the key back.
                                UnregisterHotKey(std::ptr::null_mut(), 94);
                                if RegisterHotKey(std::ptr::null_mut(), 94, m | MOD_NOREPEAT, k) != 0 {
                                    pause_packed = packed;
                                    pause_registered = true;
                                    let name = crate::shortcuts::label(packed);
                                    writeln!(log.lock().unwrap(), "HOTKEY pause applied {name} packed={packed}").ok();
                                    crate::settings_view::set_pause_label(&name);
                                    say(&format!("Pause shortcut applied: {name}. It clears the unsaved replay."), false);
                                } else {
                                    let _ = crate::file_prefs::set_pause_key(&dir, pause_packed);
                                    let (om, ok) = crate::shortcuts::split(pause_packed);
                                    pause_registered =
                                        RegisterHotKey(std::ptr::null_mut(), 94, om | MOD_NOREPEAT, ok) != 0;
                                    SendMessageW(pausekey, 0x401, pause_packed as usize, 0);
                                    say(
                                        if pause_registered {
                                            "Could not apply. Previous pause shortcut restored."
                                        } else {
                                            "Pause shortcut unavailable. The Pause button still works."
                                        },
                                        true,
                                    );
                                }
                                // Focus-safe: while a shortcut box has focus the global keys stay off
                                // (focus loss re-registers them), so typing a key never fires an action.
                                if GetFocus() == hotkey || GetFocus() == pausekey {
                                    if save_registered {
                                        UnregisterHotKey(std::ptr::null_mut(), 1);
                                        save_registered = false;
                                    }
                                    if pause_registered {
                                        UnregisterHotKey(std::ptr::null_mut(), 94);
                                        pause_registered = false;
                                    }
                                }
                            }
                            crate::ctl_ids::TEMPLATE_PREVIEW => {
                                let mut b = [0u16; 128];
                                let n = GetWindowTextW(tpledit, b.as_mut_ptr(), 128).max(0) as usize;
                                let (p, e) = template_preview(&String::from_utf16_lossy(&b[..n]));
                                crate::settings_view::set_template_feedback(&p, &e, !e.is_empty());
                            }
                            crate::ctl_ids::APPLY_TEMPLATE => {
                                let mut b = [0u16; 128];
                                let n = GetWindowTextW(tpledit, b.as_mut_ptr(), 128).max(0) as usize;
                                let text = String::from_utf16_lossy(&b[..n]);
                                let (p, e) = template_preview(&text);
                                if !e.is_empty() {
                                    crate::settings_view::set_template_feedback(&p, &format!("Not applied: {e} Nothing changed."), true);
                                    continue;
                                }
                                if let Some(dir) = crate::file_prefs::prefs_file().parent().map(|d| d.to_path_buf()) {
                                    match crate::file_prefs::set_template(&dir, &text, &crate::identity::name()) {
                                        Ok(()) => {
                                            set(namebutton, crate::file_prefs::name_label(crate::file_prefs::STYLE_TEMPLATE));
                                            crate::settings_view::set_name_example(&name_example(&dir));
                                            crate::settings_view::set_template_feedback(&p, "Applied. New clips use this template. Existing clips are not renamed.", false);
                                        }
                                        Err(e) => crate::settings_view::set_template_feedback(&p, &format!("Could not save the template: {e}. Nothing changed."), true),
                                    }
                                }
                            }
                            30 => {
                                if checking_update {
                                    continue;
                                }
                                if let Some(staged) = &staged_update {
                                    if saving.load(Ordering::SeqCst) || crate::editor::is_open() || recording || recordfinishing || recovering {
                                        set(result, "Finish saves, recording and recovery, and close the workbench, before installing.");
                                        continue;
                                    }
                                    let confirm = if crate::editor::confirm_themed(hwnd, font, &format!("Install verified update\nInstall ZimClip {} now? Capture will stop, the unsaved buffer will clear, and the app will restart.\nClips and preferences stay. Old program files are backed up.", staged.version), "Install") { IDYES } else { 0 };
                                    if confirm == IDYES {
                                        match crate::updater::apply(staged, &root) {
                                            Ok(()) => {
                                                PostThreadMessageW(
                                                    GetCurrentThreadId(),
                                                    WM_HOTKEY,
                                                    3,
                                                    0,
                                                );
                                            }
                                            Err(e) => {
                                                set(result, &format!("Update could not start: {e}"))
                                            }
                                        }
                                    }
                                } else {
                                    checking_update = true;
                                    set(updatebutton, "Checking / downloading update...");
                                    let t = update_tx.clone();
                                    let progress = update_progress_tx.clone();
                                    writeln!(log.lock().unwrap(), "UPDATE started by button").ok();
                                    thread::spawn(move || {
                                        t.send(crate::updater::stage(progress)).ok();
                                    });
                                }
                            }
                            45 => {
                                let n = SendMessageW(layoutcombo, CB_GETCURSEL, 0, 0) as usize;
                                if n < 5 {
                                    match fs::write(root.join("layout.txt"), n.to_string()) {
                                        Ok(()) => {
                                            layout = n;
                                            // Reset the page first so the layout is drawn for the page the state says.
                                            crate::polished::set_nav_page(0);
                                            crate::display::apply(
                                                hwnd,
                                                &display_items,
                                                layout,
                                                settings_open,
                                                &recentrows,
                                                large_cards,
                                            );
                                            set(result, "Layout applied to replay and Settings.");
                                        }
                                        Err(_) => set(result, "Layout could not be saved."),
                                    }
                                }
                            }
                            46 => match crate::display::choose(hwnd, &root) {
                                Ok(Some(_)) => {
                                    set(result, "Font saved. Restart app to apply it everywhere.");
                                }
                                Ok(None) => {}
                                Err(_) => set(result, "Font could not be saved."),
                            },
                            43 => {
                                if recording {
                                    crate::recording::stop(&recordtap);
                                    recording = false;
                                    recordfinishing = true;
                                    set(recordbutton, "Finishing recording...");
                                    EnableWindow(recordbutton, 0);
                                    set(result, "Finishing recording. Keep the app open.");
                                } else if !recordfinishing
                                    && !recovering
                                    && ready
                                    && !paused
                                    && !failed
                                {
                                    match crate::recording::start(
                                        &recordtap,
                                        &dir,
                                        exe.clone(),
                                        recordtx.clone(),
                                    ) {
                                        Ok(()) => {
                                            recording = true;
                                            recordbegan = Instant::now();
                                            set(recordbutton, "Stop recording");
                                            set(result,"Normal recording started. Replay clipping stays available.");
                                        }
                                        Err(e) => {
                                            set(result, &format!("Cannot start recording: {e}"))
                                        }
                                    }
                                } else {
                                    set(
                                        result,
                                        "Wait for replay capture to be ready before recording.",
                                    );
                                }
                            }
                            44 => {
                                if !recording && !recordfinishing && !recovering {
                                    recovering = true;
                                    crate::recording::recover(
                                        dir.clone(),
                                        exe.clone(),
                                        recordtx.clone(),
                                    );
                                    set(result,"Recovering unfinished recording. Raw files stay until verified.");
                                }
                            }
                            x if x == 42 || x == crate::ctl_ids::REPLAY_SLIDER || crate::ctl_ids::QUICK_LEN.contains(&x) => {
                              'len: {
                                // Slider and quick buttons give the length directly; the value button asks for it.
                                let direct: Option<u64> = if msg.wParam == crate::ctl_ids::REPLAY_SLIDER {
                                    let p = SendMessageW(lenslider, 0x400, 0, 0) as u64;
                                    Some(((p + 2) / 5 * 5).clamp(5, 300))
                                } else {
                                    crate::ctl_ids::QUICK_LEN.iter().position(|q| *q == msg.wParam).map(|i| crate::ctl_ids::QUICK_SECS[i])
                                };
                                if direct == Some(buffer_seconds) {
                                    break 'len;
                                }
                                if !crate::replay_design::confirm_clear(hwnd) {
                                    break 'len;
                                }
                                let raw = match direct {
                                    Some(v) => Some(v.to_string()),
                                    None => crate::editor::buffer_dialog(hwnd, font, buffer_seconds),
                                };
                                if let Some(raw) = raw {
                                    match raw.trim().parse::<u64>().ok().filter(|v|(5..=300).contains(v)){
                                        Some(seconds)=>{
                                            if saving.load(Ordering::SeqCst){set(result,"Wait for the current save before changing replay length.");break 'len;}
                                            if let Err(e)=fs::write(root.join("buffer-seconds.txt"),seconds.to_string()){set(result,&format!("Length not saved: {e}"));break 'len;}
                                            buffer_seconds=seconds;
                                            set(savebutton,&format!("Save last {buffer_seconds}s     {save_shortcut}"));
                                            if paused{ring=Arc::new(Mutex::new(Ring::with_seconds(buffer_seconds)));set(result,"Replay length saved. Resume starts a fresh buffer.");}else{PostThreadMessageW(GetCurrentThreadId(),WM_HOTKEY,4,0);}
                                        },None=>set(result,"Use a whole number from 5 to 300 seconds. Length unchanged.")
                                    }
                                }
                            }
                              sync_len(lenslider, durationbutton, &lenquick, buffer_seconds);
                            }
                            x if x == crate::ctl_ids::TAGLINE_EDIT => {
                                let mut buf = [0u16; 96];
                                let n = GetWindowTextW(taglineedit, buf.as_mut_ptr(), 96).max(0) as usize;
                                let text = String::from_utf16_lossy(&buf[..n]);
                                if text.trim() == crate::identity::tagline() {
                                    continue;
                                }
                                match crate::identity::save_tagline(&root, &text) {
                                    Ok(t) => {
                                        set(taglineedit, &t);
                                        set(tagline, &t);
                                        InvalidateRect(hwnd, std::ptr::null(), 1);
                                        set(result, "Tagline saved.");
                                    }
                                    Err(e) => {
                                        set(taglineedit, &crate::identity::tagline());
                                        set(result, &format!("Tagline not saved: {e}"));
                                    }
                                }
                            }
                            95 => {
                                let mut buf = [0u16; 64];
                                let n = GetWindowTextW(nameedit, buf.as_mut_ptr(), 64).max(0) as usize;
                                let name = String::from_utf16_lossy(&buf[..n]);
                                if name.trim() == crate::identity::name() {
                                    continue;
                                }
                                match crate::identity::save(&root, &name) {
                                    Ok(name) => {
                                        set(nameedit, &name);
                                        set(appheading, &name);
                                        crate::ui::apply_identity(hwnd, false);
                                        let editor = FindWindowW(wide("ZimClipEditor").as_ptr(), std::ptr::null());
                                        if !editor.is_null() {
                                            crate::ui::apply_identity(editor, true);
                                        }
                                        set(result, "Display name saved. Files and folders are unchanged.");
                                    }
                                    Err(e) => {
                                        set(nameedit, &crate::identity::name());
                                        set(result, &format!("Name not saved: {e}"));
                                    }
                                }
                            }
                            41 => {
                                if let Some(choice) = crate::editor::icon_dialog(hwnd, font, crate::identity::icon_choice()) {
                                    let saved = if choice == "upload" {
                                        match crate::icon_upload::pick(hwnd) {
                                            Ok(Some(px)) => Some(crate::identity::save_icon(&root, crate::identity::custom_choice(), Some(&px))),
                                            Ok(None) => None,
                                            Err(e) => Some(Err(e)),
                                        }
                                    } else {
                                        let n = crate::identity::parse_icon(&choice);
                                        let invalid = choice != "default" && n == 0;
                                        if invalid || (n == crate::identity::icon_choice() && n != 0) {
                                            None
                                        } else {
                                            Some(crate::identity::save_icon(&root, n, None))
                                        }
                                    };
                                    match saved {
                                        Some(Ok(())) => {
                                            crate::ui::apply_identity(hwnd, false);
                                            let editor = FindWindowW(wide("ZimClipEditor").as_ptr(), std::ptr::null());
                                            if !editor.is_null() {
                                                crate::ui::apply_identity(editor, true);
                                            }
                                            InvalidateRect(hwnd, std::ptr::null(), 0);
                                            set(result, "App icon saved.");
                                        }
                                        Some(Err(e)) => set(result, &format!("Icon not saved: {e}")),
                                        None => {}
                                    }
                                }
                            }
                            40 => {
                                details_open = !details_open;
                                // One owner for Details: the window property. Layout code reads it,
                                // never the visibility of a control that a page switch may hide.
                                SetPropW(hwnd, wide("compact-expanded").as_ptr(), details_open as usize as _);
                                crate::nav::set_home_sub(0);
                                set(
                                    detailsbutton,
                                    if details_open {
                                        "Hide details"
                                    } else {
                                        "Details"
                                    },
                                );
                                ShowWindow(detail, if details_open { SW_SHOW } else { SW_HIDE });
                                ShowWindow(
                                    audiolabel,
                                    if details_open { SW_SHOW } else { SW_HIDE },
                                );
                                ShowWindow(warning, if details_open { SW_HIDE } else { SW_SHOW });
                                crate::display::apply(
                                    hwnd,
                                    &display_items,
                                    layout,
                                    settings_open,
                                    &recentrows,
                                    large_cards,
                                );
                            }
                            37 | 38 | 39 => {
                                let offset =
                                    GetPropW(hwnd, wide("library-offset").as_ptr()) as usize;
                                let count = GetPropW(hwnd, wide("library-count").as_ptr()) as usize;
                                let next = match msg.wParam {
                                    37 => offset.saturating_sub(1),
                                    38 => offset.saturating_add(1),
                                    _ => msg.lParam.max(0) as usize,
                                }
                                .min(count.saturating_sub(crate::library_view::capacity_for(hwnd)));
                                SetPropW(hwnd, wide("library-offset").as_ptr(), next as _);
                                refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                                crate::thumbs::request(recentpaths.clone(), thumbtx.clone());
                            }
                            85 => {
                                if let Err(e) = crate::library_view::manage_tags(hwnd, font, &dir) {
                                    set(result, &e);
                                }
                                refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                            }
                            36 | 90 if import_pending => {
                                set(result, "Settings are locked after an import. Reopen ZimClip first.");
                            }
                            36 | 90 => {
                                // Larger (90) always selects large; Compact (36) selects compact in the
                                // polished UI, and the single legacy button toggles.
                                large_cards = if msg.wParam == 90 {
                                    true
                                } else {
                                    false
                                };
                                fs::write(
                                    root.join("library.ini"),
                                    if large_cards { "large=1" } else { "large=0" },
                                )
                                .ok();
                                density_labels(largebutton, sizebutton, large_cards);
                                InvalidateRect(GetDlgItem(hwnd, 36), std::ptr::null(), 0);
                                InvalidateRect(GetDlgItem(hwnd, 90), std::ptr::null(), 0);
                                refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                                crate::display::apply(
                                    hwnd,
                                    &display_items,
                                    layout,
                                    settings_open,
                                    &recentrows,
                                    large_cards,
                                );
                            }
                            14 => {
                                let fade = crate::ui::transition::snapshot(hwnd);
                                SendMessageW(hwnd, WM_SETREDRAW, 0, 0);
                                settings_open = !settings_open;
                                set(
                                    viewheading,
                                    if settings_open {
                                        "SETTINGS"
                                    } else {
                                        "LIVE BUFFER"
                                    },
                                );
                                for c in &main_controls {
                                    ShowWindow(*c, if settings_open { SW_HIDE } else { SW_SHOW });
                                }
                                ShowWindow(
                                    detail,
                                    if !settings_open && details_open {
                                        SW_SHOW
                                    } else {
                                        SW_HIDE
                                    },
                                );
                                ShowWindow(
                                    audiolabel,
                                    if !settings_open && details_open {
                                        SW_SHOW
                                    } else {
                                        SW_HIDE
                                    },
                                );
                                ShowWindow(
                                    warning,
                                    if !settings_open && !details_open {
                                        SW_SHOW
                                    } else {
                                        SW_HIDE
                                    },
                                );
                                for c in &settings_controls {
                                    ShowWindow(*c, if settings_open { SW_SHOW } else { SW_HIDE });
                                }
                                set(
                                    GetDlgItem(hwnd, 14),
                                    if settings_open {
                                        "Back to replay"
                                    } else {
                                        "Settings"
                                    },
                                );
                                for (i, c) in recentrows.iter().enumerate() {
                                    ShowWindow(
                                        *c,
                                        if !settings_open && i < recentpaths.len() {
                                            SW_SHOW
                                        } else {
                                            SW_HIDE
                                        },
                                    );
                                }
                                crate::nav::set_recover(crate::recording::pending(&dir) && !recording);
                                ShowWindow(
                                    recoverbutton,
                                    if crate::nav::recover_visible(
                                        crate::recording::pending(&dir) && !recording,
                                        settings_open,
                                        crate::polished::sidebar()
                                            || crate::nav::compact_page(layout, crate::polished::enabled(), false).is_some(),
                                        crate::polished::nav_page(),
                                    ) {
                                        SW_SHOW
                                    } else {
                                        SW_HIDE
                                    },
                                );
                                ShowWindow(
                                    libraryscroll,
                                    if settings_open { SW_HIDE } else { SW_SHOW },
                                );
                                ShowWindow(
                                    sizebutton,
                                    if settings_open { SW_HIDE } else { SW_SHOW },
                                );
                                ShowWindow(
                                    openclipsbutton,
                                    if settings_open { SW_HIDE } else { SW_SHOW },
                                );
                                set(
                                    rightheading,
                                    if settings_open {
                                        "PERSONALIZE"
                                    } else {
                                        "RECENT CLIPS"
                                    },
                                );
                                set(
                                    righthint,
                                    if settings_open {
                                        "Make it yours. Keep it light."
                                    } else {
                                        "Click a replay to open it."
                                    },
                                );
                                if !settings_open {
                                    SetPropW(hwnd, wide("display-layout").as_ptr(), layout as _);
                                    refresh_recent(&dir, &recentrows, &mut recentpaths, true);
                                }
                                crate::display::apply(
                                    hwnd,
                                    &display_items,
                                    layout,
                                    settings_open,
                                    &recentrows,
                                    large_cards,
                                );
                                SendMessageW(hwnd, WM_SETREDRAW, 1, 0);
                                RedrawWindow(
                                    hwnd,
                                    std::ptr::null(),
                                    std::ptr::null_mut(),
                                    RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_NOERASE,
                                );
                                if let Some(f) = fade {
                                    crate::ui::transition::play(hwnd, f);
                                }
                            }
                            20 | 21 | 23 => {
                                if paused || (!ready && !failed) || saving.load(Ordering::SeqCst) {
                                    SendMessageW(
                                        rescombo,
                                        CB_SETCURSEL,
                                        match capture_width {
                                            854 => 0,
                                            1920 => 2,
                                            _ => 1,
                                        },
                                        0,
                                    );
                                    SendMessageW(
                                        pathcombo,
                                        CB_SETCURSEL,
                                        if desired_compat { 0 } else { 1 },
                                        0,
                                    );
                                    SendMessageW(
                                        fpscombo,
                                        CB_SETCURSEL,
                                        if capture_fps == 60 { 1 } else { 0 },
                                        0,
                                    );
                                    set(result,"Resume capture and wait for startup/save before changing video settings.");
                                    continue;
                                }
                                if !crate::replay_design::confirm_clear(hwnd) {
                                    SendMessageW(
                                        rescombo,
                                        CB_SETCURSEL,
                                        match capture_width {
                                            854 => 0,
                                            1920 => 2,
                                            _ => 1,
                                        },
                                        0,
                                    );
                                    SendMessageW(
                                        pathcombo,
                                        CB_SETCURSEL,
                                        if desired_compat { 0 } else { 1 },
                                        0,
                                    );
                                    SendMessageW(
                                        fpscombo,
                                        CB_SETCURSEL,
                                        if capture_fps == 60 { 1 } else { 0 },
                                        0,
                                    );
                                    continue;
                                }
                                capture_width = match SendMessageW(rescombo, CB_GETCURSEL, 0, 0) {
                                    0 => 854,
                                    2 => 1920,
                                    _ => 1280,
                                };
                                capture_fps = if SendMessageW(fpscombo, CB_GETCURSEL, 0, 0) == 1 {
                                    60
                                } else {
                                    30
                                };
                                compat = SendMessageW(pathcombo, CB_GETCURSEL, 0, 0) == 0;
                                desired_compat = compat;
                                PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 4, 0);
                            }
                            22 => {
                                let i =
                                    SendMessageW(accentcombo, CB_GETCURSEL, 0, 0).max(0) as usize;
                                crate::ui::set_accent(i);
                                crate::ui::apply_identity(hwnd, false);
                                fs::write(root.join("appearance.ini"), i.to_string()).ok();
                                let editor =
                                    FindWindowW(wide("ZimClipEditor").as_ptr(), std::ptr::null());
                                if !editor.is_null() {
                                    crate::ui::apply_identity(editor, true);
                                    InvalidateRect(editor, std::ptr::null(), 0);
                                    EnumChildWindows(editor, Some(crate::ui::invalidate_child), 0);
                                }
                                InvalidateRect(hwnd, std::ptr::null(), 1);
                                EnumChildWindows(hwnd, Some(crate::ui::invalidate_child), 0);
                            }
                            26 | 27 => {
                                let (control, value, atomic) = if msg.wParam == 26 {
                                    (sysslider, sysvalue, &gain)
                                } else {
                                    (micslider, micvalue, &mic_gain)
                                };
                                let db = SendMessageW(control, 0x400, 0, 0) as i32;
                                atomic.store(db, Ordering::Relaxed);
                                writeln!(
                                    log.lock().unwrap(),
                                    "GAIN control={} db={db}",
                                    msg.wParam
                                )
                                .ok();
                                set(value, &format!("+{db} dB"));
                            }
                            81 => {
                                refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                                crate::thumbs::request(recentpaths.clone(), thumbtx.clone());
                                set(result, "Clips refreshed. Search and tag filters are kept.");
                            }
                            79 | 80 => {
                                if recording
                                    || recordfinishing
                                    || recovering
                                    || saving.load(Ordering::SeqCst)
                                    || crate::editor::is_open()
                                    || (msg.wParam == 80 && crate::recording::pending(&dir))
                                {
                                    set(result,"Finish recording/saving, recover pending recording and close workbench before changing devices or folder.");
                                    continue;
                                }
                                if msg.wParam == 79 {
                                    if !crate::replay_design::confirm_clear(hwnd) {
                                        continue;
                                    }
                                    if let Some(c) = crate::audio_devices::dialog(hwnd, font) {
                                        match crate::audio_devices::save(&root, c) {
                                            Ok(()) => {
                                                PostThreadMessageW(
                                                    GetCurrentThreadId(),
                                                    WM_HOTKEY,
                                                    4,
                                                    0,
                                                );
                                            }
                                            Err(e) => set(result, &e),
                                        }
                                    }
                                } else {
                                    match crate::save_folder::choose(hwnd, &dir) {
                                        Ok(Some(folder)) => {
                                            match crate::save_folder::save(&root, &folder) {
                                                Ok(()) => {
                                                    dir = folder;
                                                    SetPropW(
                                                        hwnd,
                                                        wide("library-offset").as_ptr(),
                                                        std::ptr::null_mut(),
                                                    );
                                                    *crate::library::state().lock().unwrap() =
                                                        Default::default();
                                                    refresh_recent(
                                                        &dir,
                                                        &recentrows,
                                                        &mut recentpaths,
                                                        !settings_open,
                                                    );
                                                    crate::thumbs::request(
                                                        recentpaths.clone(),
                                                        thumbtx.clone(),
                                                    );
                                                    set(result,&format!("Videos save to {}. Existing files were not moved. Filters kept.",dir.display()));
                                                }
                                                Err(e) => set(result, &e),
                                            }
                                        }
                                        Ok(None) => {}
                                        Err(e) => set(result, &e),
                                    }
                                }
                            }
                            73 => {
                                if recording
                                    || recordfinishing
                                    || recovering
                                    || saving.load(Ordering::SeqCst)
                                    || crate::editor::is_open()
                                {
                                    set(result,"Finish recording/saving and close workbench before changing audio.");
                                } else if !crate::replay_design::confirm_clear(hwnd) {
                                    continue;
                                } else if let Some(apps) = crate::editor::comms_dialog(hwnd, font) {
                                    match crate::comms::save(&root, &apps) {
                                        Ok(()) => {
                                            set(
                                                commsbutton,
                                                if crate::comms::enabled() {
                                                    "Voice chat apps..."
                                                } else {
                                                    "Voice chat: OFF"
                                                },
                                            );
                                            PostThreadMessageW(
                                                GetCurrentThreadId(),
                                                WM_HOTKEY,
                                                4,
                                                0,
                                            );
                                        }
                                        Err(e) => set(result, &e),
                                    }
                                }
                            }
                            72 | 76 | 77 | 78 => {
                                if let Err(e) = crate::library_view::read_toolbar(hwnd, &root) {
                                    set(result, &e);
                                }
                                SetPropW(
                                    hwnd,
                                    wide("library-offset").as_ptr(),
                                    std::ptr::null_mut(),
                                );
                                refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                                crate::thumbs::request(recentpaths.clone(), thumbtx.clone());
                            }
                            70 => {
                                let mut s = crate::library::state().lock().unwrap();
                                s.active = !s.active;
                                if !s.active {
                                    s.paths.clear();
                                }
                                drop(s);
                            }
                            71 => {
                                let targets: Vec<_> = crate::library::state()
                                    .lock()
                                    .unwrap()
                                    .paths
                                    .iter()
                                    .cloned()
                                    .collect();
                                if !targets.is_empty() {
                                    if crate::editor::is_open() {
                                        set(result, "Close the workbench before deleting clips.");
                                    } else {
                                        let matching = recent_files(&dir);
                                        let hidden = targets
                                            .iter()
                                            .filter(|p| !matching.contains(p))
                                            .count();
                                        let summary = if hidden == 0 {
                                            format!("{} selected clips", targets.len())
                                        } else {
                                            format!("{} selected clips, including {hidden} hidden by this filter", targets.len())
                                        };
                                        let answer = crate::editor::confirm_clip_delete(
                                            hwnd, font, &summary,
                                        );
                                        if answer {
                                            let mut done = 0;
                                            let mut errors = Vec::new();
                                            for p in targets {
                                                match crate::library::trash(&p, &dir) {
                                                    Ok(_) => {
                                                        done += 1;
                                                        crate::library::state()
                                                            .lock()
                                                            .unwrap()
                                                            .paths
                                                            .remove(&p);
                                                    }
                                                    Err(e) => errors.push(format!(
                                                        "{}: {e}",
                                                        p.file_name()
                                                            .unwrap_or_default()
                                                            .to_string_lossy()
                                                    )),
                                                }
                                            }
                                            set(result,&format!("Moved {done} clips to Clips\\Trash. {} could not be moved.",errors.len()));
                                            for e in errors {
                                                writeln!(log.lock().unwrap(), "TRASH failed {e}")
                                                    .ok();
                                            }
                                            refresh_recent(
                                                &dir,
                                                &recentrows,
                                                &mut recentpaths,
                                                !settings_open,
                                            );
                                            crate::thumbs::request(
                                                recentpaths.clone(),
                                                thumbtx.clone(),
                                            );
                                        }
                                    }
                                }
                            }
                            220..=224 => {
                                if let Some(p) = recentpaths.get(msg.wParam - 220).cloned() {
                                    match crate::favourites::toggle(&p) {
                                        Ok(on) => {
                                            set(result, if on { "Added to favourites." } else { "Removed from favourites." });
                                            refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                                        }
                                        Err(e) => set(result, &format!("Favourite not saved: {e}")),
                                    }
                                }
                            }
                            200..=204 | 210..=214 => {
                                let from_trash = msg.wParam >= 210;
                                if let Some(p) = recentpaths.get(msg.wParam - if from_trash { 210 } else { 200 }).cloned() {
                                    let menu = CreatePopupMenu();
                                    for (id, text) in [
                                        (301, "Open clip"),
                                        (302, "Select clip"),
                                        (303, "Delete clip"),
                                        (304, "Open clips folder"),
                                        (305, "Manage tags..."),
                                    ] {
                                        AppendMenuW(menu, MF_OWNERDRAW, id, wide(text).as_ptr());
                                    }
                                    let assignment = CreatePopupMenu();
                                    let reusable =
                                        crate::catalog::reusable(&dir).unwrap_or_default();
                                    let assigned = crate::catalog::tags(&dir)
                                        .get(&p.file_name().unwrap().to_string_lossy().to_string())
                                        .cloned()
                                        .unwrap_or_default();
                                    let names: Vec<String> = reusable.keys().cloned().collect();
                                    let tag_labels: Vec<Vec<u16>> =
                                        names.iter().map(|n| wide(n)).collect();
                                    for (i, name) in names.iter().enumerate() {
                                        let checked = assigned
                                            .split(',')
                                            .any(|t| t.trim().eq_ignore_ascii_case(name));
                                        AppendMenuW(
                                            assignment,
                                            MF_OWNERDRAW
                                                | if checked { MF_CHECKED } else { MF_UNCHECKED },
                                            500 + i,
                                            tag_labels[i].as_ptr(),
                                        );
                                    }
                                    AppendMenuW(assignment, MF_SEPARATOR, 0, std::ptr::null());
                                    AppendMenuW(
                                        assignment,
                                        MF_OWNERDRAW,
                                        305,
                                        wide("Manage tags...").as_ptr(),
                                    );
                                    let assign_label = wide("Assign tags");
                                    InsertMenuW(
                                        menu,
                                        1,
                                        MF_BYPOSITION | MF_POPUP | MF_OWNERDRAW,
                                        assignment as usize,
                                        assign_label.as_ptr(),
                                    );
                                    let mut pt = POINT::default();
                                    GetCursorPos(&mut pt);
                                    SetForegroundWindow(hwnd);
                                    // The trash icon goes straight to the same confirmed delete as "Delete clip".
                                    let cmd = if from_trash { 303 } else { crate::menu_round::track(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, hwnd) as usize };
                                    DestroyMenu(menu);
                                    match cmd {
                                        301 => crate::editor::open(p, exe.clone()),
                                        302 => crate::library::toggle(p),
                                        303 => {
                                            if crate::editor::is_open() {
                                                set(
                                                    result,
                                                    "Close the workbench before deleting clips.",
                                                );
                                            } else if crate::editor::confirm_clip_delete(
                                                hwnd,
                                                font,
                                                &p.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy(),
                                            ) {
                                                match crate::library::trash(&p, &dir) {
                                                    Ok(_) => {
                                                        crate::library::state()
                                                            .lock()
                                                            .unwrap()
                                                            .paths
                                                            .remove(&p);
                                                        set(result, "Clip moved to Clips\\Trash.");
                                                        refresh_recent(
                                                            &dir,
                                                            &recentrows,
                                                            &mut recentpaths,
                                                            !settings_open,
                                                        );
                                                        crate::thumbs::request(
                                                            recentpaths.clone(),
                                                            thumbtx.clone(),
                                                        );
                                                    }
                                                    Err(e) => {
                                                        set(result, &format!("Clip unchanged: {e}"))
                                                    }
                                                }
                                            }
                                        }
                                        305 => {
                                            if let Err(e) =
                                                crate::library_view::manage_tags(hwnd, font, &dir)
                                            {
                                                set(result, &e);
                                            }
                                            refresh_recent(
                                                &dir,
                                                &recentrows,
                                                &mut recentpaths,
                                                !settings_open,
                                            );
                                        }
                                        500..=29999 => {
                                            if let Some(name) = names.get(cmd as usize - 500) {
                                                let mut tags: Vec<String> = assigned
                                                    .split(',')
                                                    .map(str::trim)
                                                    .filter(|s| !s.is_empty())
                                                    .map(str::to_string)
                                                    .collect();
                                                if let Some(i) = tags
                                                    .iter()
                                                    .position(|s| s.eq_ignore_ascii_case(name))
                                                {
                                                    tags.remove(i);
                                                } else {
                                                    tags.push(name.clone());
                                                }
                                                match crate::catalog::set_tags(&p, &tags.join(", "))
                                                {
                                                    Ok(()) => set(result, "Tags saved."),
                                                    Err(e) => set(result, &e),
                                                }
                                                refresh_recent(
                                                    &dir,
                                                    &recentrows,
                                                    &mut recentpaths,
                                                    !settings_open,
                                                );
                                            }
                                        }
                                        304 => {
                                            let _ = Command::new("explorer.exe").arg(&dir).spawn();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            110 | 111 => {
                                let fade = if crate::polished::nav_page() == (msg.wParam - 110) as usize {
                                    None
                                } else {
                                    crate::ui::transition::snapshot(hwnd)
                                };
                                crate::polished::set_nav_page((msg.wParam - 110) as usize);
                                // Fill the rows for the new page's capacity before they are shown.
                                refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                                crate::display::apply(
                                    hwnd,
                                    &display_items,
                                    layout,
                                    settings_open,
                                    &recentrows,
                                    large_cards,
                                );
                                InvalidateRect(hwnd, std::ptr::null(), 0);
                                if let Some(f) = fade {
                                    UpdateWindow(hwnd);
                                    crate::ui::transition::play(hwnd, f);
                                }
                            }
                            x if x == crate::ctl_ids::HOME_SUBPAGE => {
                                crate::nav::set_home_sub(1 - crate::nav::home_sub());
                                crate::display::apply(
                                    hwnd,
                                    &display_items,
                                    layout,
                                    settings_open,
                                    &recentrows,
                                    large_cards,
                                );
                                InvalidateRect(hwnd, std::ptr::null(), 0);
                            }
                            114 => {
                                // Native popup menu: keyboard reachable (Alt+Down or Enter on the button).
                                let cards = crate::nav::cards();
                                let mut labels: Vec<Vec<u16>> = vec![];
                                let mut items: Vec<(usize, u32, usize)> = vec![]; // (command, flags, label index)
                                let mut add = |cmd: usize, flags: u32, text: String, labels: &mut Vec<Vec<u16>>| {
                                    labels.push(wide(&text));
                                    items.push((cmd, flags, labels.len() - 1));
                                };
                                for (i, c) in crate::nav::Card::ALL.into_iter().enumerate() {
                                    let pos = cards.order.iter().position(|x| *x == c).unwrap();
                                    if !c.essential() {
                                        let t = if cards.is_hidden(c) { "Show" } else { "Hide" };
                                        add(7100 + i, 0, format!("{t} {} card", c.name()), &mut labels);
                                    }
                                    add(7110 + i, if pos == 0 { MF_GRAYED } else { 0 }, format!("Move {} card up", c.name()), &mut labels);
                                    add(7120 + i, if pos == 2 { MF_GRAYED } else { 0 }, format!("Move {} card down", c.name()), &mut labels);
                                }
                                add(7199, 0, "Reset Home layout".into(), &mut labels);
                                let menu = CreatePopupMenu();
                                for (cmd, flags, li) in &items {
                                    if *cmd == 7199 {
                                        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                                    }
                                    AppendMenuW(menu, MF_OWNERDRAW | flags, *cmd, labels[*li].as_ptr());
                                }
                                let mut r = RECT::default();
                                GetWindowRect(GetDlgItem(hwnd, 114), &mut r);
                                SetForegroundWindow(hwnd);
                                let cmd = crate::menu_round::track(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, r.left, r.bottom, hwnd) as usize;
                                DestroyMenu(menu);
                                let mut next = cards;
                                let ok = match cmd {
                                    7199 => {
                                        next = crate::nav::HomeCards::default();
                                        true
                                    }
                                    7100..=7102 => next.toggle_hidden(crate::nav::Card::ALL[cmd - 7100]),
                                    7110..=7112 => next.shift(crate::nav::Card::ALL[cmd - 7110], -1),
                                    7120..=7122 => next.shift(crate::nav::Card::ALL[cmd - 7120], 1),
                                    _ => false,
                                };
                                if ok && next != cards {
                                    crate::nav::set_cards(next);
                                    if let Err(e) = crate::nav::save_cards(&root, &next) {
                                        set(result, &format!("Could not save the Home layout: {e}. The change applies for now only."));
                                    }
                                    crate::display::apply(hwnd, &display_items, layout, settings_open, &recentrows, large_cards);
                                    InvalidateRect(hwnd, std::ptr::null(), 0);
                                }
                            }
                            112 => {
                                // Navigation never picks a clip. Use the selection only when it is
                                // exactly one clip; otherwise say how to choose one.
                                let chosen = crate::library::state().lock().unwrap().paths.clone();
                                if chosen.len() == 1 {
                                    if let Some(p) = chosen.iter().next() { crate::editor::open(p.clone(), exe.clone()); }
                                } else {
                                    set(result, "Choose a clip first: click one on the Clips page to open it in the Editor.");
                                }
                            }
                            113 => {
                                if !settings_open {
                                    PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 14, 0);
                                }
                            }
                            125 => {
                                if !settings_open {
                                    PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 111, 0);
                                }
                            }
                            100..=104 | 120..=123 => {
                                let picked = if msg.wParam >= 120 {
                                    crate::home_stats::home_path((msg.wParam - 120) as usize)
                                } else {
                                    recentpaths.get((msg.wParam - 100) as usize).cloned()
                                };
                                if let Some(p) = picked.as_ref() {
                                    if crate::library::state().lock().unwrap().active {
                                        crate::library::toggle(p.clone());
                                    } else {
                                        crate::editor::open(p.clone(), exe.clone());
                                    }
                                }
                            }
                            2 => {
                                let _ = Command::new("explorer.exe").arg(&dir).spawn();
                            }
                            3 => {
                                writeln!(
                                    log.lock().unwrap(),
                                    "EXIT requested saving={} editor={}",
                                    saving.load(Ordering::SeqCst),
                                    crate::editor::is_saving()
                                )
                                .ok();
                                crate::background::reveal(hwnd);
                                if crate::editor::is_saving() {
                                    set(result, "A trimmed copy is saving. Wait before quitting.");
                                    continue;
                                }
                                if saving.load(Ordering::SeqCst) {
                                    set(result, "A clip is saving. Wait a moment, then quit.");
                                } else {
                                    let final_settings = crate::capture_settings::CaptureSettings {
                                        monitor: index,
                                        compatibility: desired_compat,
                                        width: capture_width,
                                        fps: capture_fps,
                                        system_audio: audio_on,
                                        microphone: mic_on,
                                        system_gain: gain.load(Ordering::Relaxed),
                                        microphone_gain: mic_gain.load(Ordering::Relaxed),
                                    };
                                    if let Err(e) = if import_pending { Ok(()) } else { final_settings.save(&root) } {
                                        set(result,"Cannot save capture settings. See Diagnostics before exiting.");
                                        writeln!(
                                            log.lock().unwrap(),
                                            "PREFERENCES exit save failed: {e}"
                                        )
                                        .ok();
                                        continue;
                                    }
                                    let _ = child.kill();
                                    let _ = child.wait();
                                    for (id, _) in keys {
                                        UnregisterHotKey(std::ptr::null_mut(), id);
                                    }
                                    UnregisterHotKey(std::ptr::null_mut(), 94);
                                    CloseHandle(job);
                                    CloseHandle(instance);
                                    crate::background::remove(hwnd);
                                    DestroyWindow(hwnd);
                                    DeleteObject(font);
                                    DeleteObject(titlefont);
                                    return Ok(());
                                }
                            }
                            10 => {
                                if saving.load(Ordering::SeqCst) {
                                    set(result, "Wait for the current save before pausing.");
                                    continue;
                                }
                                if paused {
                                    PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 4, 0);
                                    continue;
                                }
                                let _ = child.kill();
                                let _ = child.wait();
                                ring = Arc::new(Mutex::new(Ring::with_seconds(buffer_seconds)));
                                paused = true;
                                failed = true;
                                ready = false;
                                set(capturebutton, "Resume replay");
                                set(status, "Replay paused");
                                set(
                                    simplehint,
                                    "Capture is off. Resume to start a fresh replay.",
                                );
                                set(warning, "");
                                set(
                                    detail,
                                    "Video and audio capture stopped. Unsaved history cleared.",
                                );
                                set(
                                    audiolabel,
                                    "No audio is being captured while replay is paused.",
                                );
                                set(result, "Paused. The unsaved replay was cleared; saved clips are untouched.");
                                crate::replay_design::buffer(bufferbar, 0, buffer_seconds as usize);
                                crate::replay_design::attention(hwnd);
                                for c in [audiobutton, micbutton, commsbutton] {
                                    InvalidateRect(c, std::ptr::null(), 0);
                                }
                                EnableWindow(savebutton, 0);
                                EnableWindow(restartbutton, 1);
                                writeln!(log.lock().unwrap(), "REPLAY PAUSED").ok();
                            }
                            4 => {
                                if !ready && !failed {
                                    set(result,"Startup is still in progress. Automatic fallback will run.");
                                    continue;
                                }
                                if saving.load(Ordering::SeqCst) {
                                    set(result, "Wait for the current save before restarting.");
                                    continue;
                                }
                                set(
                                    result,
                                    "Capture restarted. A fresh replay buffer is filling.",
                                );
                                paused = false;
                                set(capturebutton, "Pause replay");
                                let _ = child.kill();
                                let _ = child.wait();
                                ring = Arc::new(Mutex::new(Ring::with_seconds(buffer_seconds)));
                                mic_status = Arc::new(Mutex::new(
                                    if mic_on { "Starting mic..." } else { "Mic OFF" }.into(),
                                ));
                                audio_status =
                                    Arc::new(Mutex::new(String::from("Starting system audio...")));
                                writeln!(
                                    log.lock().unwrap(),
                                    "MANUAL RESTART compatibility={compat}"
                                )
                                .ok();
                                EnableWindow(restartbutton, 0);
                                child = start(
                                    &exe,
                                    index,
                                    compat,
                                    audio_status.clone(),
                                    gain.clone(),
                                    log.clone(),
                                    audio_on,
                                    capture_width,
                                    capture_fps,
                                    mic_on,
                                    mic_gain.clone(),
                                    mic_status.clone(),
                                )
                                .map_err(|e| e.to_string())?;
                                if AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
                                    let _ = child.kill();
                                    return Err("Cannot manage restarted capture.".into());
                                }
                                attach_reader(
                                    &mut child,
                                    ring.clone(),
                                    log.clone(),
                                    recordtap.clone(),
                                );
                                began = Instant::now();
                                ready = false;
                                failed = false;
                                EnableWindow(savebutton, 0);
                                set(status, "Restarting...");
                                set(detail,"Replay buffer is warming up. Previous unsaved history was cleared.");
                            }
                            7 | 8 | 9 | 12 => {
                                // Existing restart can block dispatch. Do not leave decorative
                                // hover/press timers armed across its modal or helper shutdown.
                                crate::ui::motion::settle_thread();
                                if paused {
                                    set(result, "Resume replay before changing capture setup.");
                                    continue;
                                }
                                if !ready && !failed {
                                    set(result, "Wait for startup before changing test mode.");
                                    continue;
                                }
                                if saving.load(Ordering::SeqCst) {
                                    set(result, "Wait for save before changing test mode.");
                                    continue;
                                }
                                if (msg.wParam == 7 || msg.wParam == 12)
                                    && !crate::replay_design::confirm_clear(hwnd)
                                {
                                    continue;
                                }
                                if msg.wParam == 7 {
                                    audio_on = !audio_on;
                                    set(
                                        audiobutton,
                                        if audio_on {
                                            "System audio ON"
                                        } else {
                                            "System audio OFF"
                                        },
                                    );
                                } else if msg.wParam == 8 {
                                    capture_width = if capture_width == 1280 { 1920 } else { 1280 };
                                    set(
                                        resbutton,
                                        if capture_width == 1280 {
                                            "720p max"
                                        } else {
                                            "1080p max"
                                        },
                                    );
                                }
                                if msg.wParam == 12 {
                                    mic_on = !mic_on;
                                    set(micbutton, if mic_on { "Mic ON" } else { "Mic OFF" });
                                }
                                if msg.wParam == 9 {
                                    compat = !compat;
                                    desired_compat = compat;
                                    set(
                                        pathbutton,
                                        if compat {
                                            "CPU compatibility"
                                        } else {
                                            "GPU conversion"
                                        },
                                    );
                                }
                                PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 4, 0);
                            }
                            13 => {
                                let v = (mic_gain.load(Ordering::Relaxed) + 6) % 25;
                                mic_gain.store(v, Ordering::Relaxed);
                                set(micgainbutton, &format!("Mic +{v}dB"));
                            }
                            6 => {
                                let next = (gain.load(Ordering::Relaxed) + 6) % 30;
                                gain.store(next, Ordering::Relaxed);
                                set(gainbutton, &format!("Boost +{next}dB"));
                                set(result,"Recorder boost changed for new audio only. Windows volume unchanged.");
                            }
                            5 => {
                                crate::diagnostics_view::show(hwnd, &root, font);
                            }
                            _ => {}
                        }
                    } else {
                        if (GetFocus() == hotkey || GetFocus() == pausekey)
                            && matches!(
                                msg.message,
                                WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP
                            )
                        {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        } else if msg.message == WM_KEYDOWN && msg.wParam == 13 {
                            // No implicit default Save button. Enter belongs to the
                            // focused button, card or dropdown, never the dialog.
                            let focused = GetFocus();
                            let mut class = [0u16; 80];
                            GetClassNameW(focused, class.as_mut_ptr(), 80);
                            let class = String::from_utf16_lossy(&class)
                                .trim_end_matches('\0')
                                .to_string();
                            if class.eq_ignore_ascii_case("BUTTON") {
                                let id = GetDlgCtrlID(focused);
                                if [1, 7, 12, 10, 43, 14, 4, 5, 40, 44, 73].contains(&id)
                                    && IsWindowEnabled(focused) != 0
                                {
                                    SendMessageW(
                                        GetParent(focused),
                                        WM_COMMAND,
                                        id as usize,
                                        focused as isize,
                                    );
                                } else {
                                    SendMessageW(focused, BM_CLICK, 0, 0);
                                }
                            } else {
                                TranslateMessage(&msg);
                                DispatchMessageW(&msg);
                            }
                        } else if IsDialogMessageW(hwnd, &msg) == 0 {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                }
                if !pause_registered
                    && GetFocus() != hotkey
                    && GetFocus() != pausekey
                    && pause_retry.elapsed() > Duration::from_secs(5)
                {
                    pause_retry = Instant::now();
                    let (pm, pk) = crate::shortcuts::split(pause_packed);
                    pause_registered = RegisterHotKey(std::ptr::null_mut(), 94, pm | MOD_NOREPEAT, pk) != 0;
                }
                if !save_registered
                    && GetFocus() != hotkey
                    && GetFocus() != pausekey
                    && hotkey_retry.elapsed() > Duration::from_secs(5)
                {
                    hotkey_retry = Instant::now();
                    save_registered = RegisterHotKey(
                        std::ptr::null_mut(),
                        1,
                        save_modifiers | MOD_NOREPEAT,
                        save_key,
                    ) != 0;
                    if save_registered {
                        set(
                            savebutton,
                            &format!("Save last {buffer_seconds}s     {save_shortcut}"),
                        );
                        set(
                            shortcutlabel,
                            &format!("Save shortcut active: {save_shortcut}"),
                        );
                    }
                }
                if crate::home_stats::set_capture(buffer_seconds as usize, capture_width as u32, capture_fps as u32, desired_compat) {
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                if let Ok(v) = notice_rx.try_recv() {
                    pending_notice = Some(v);
                }
                // Ask only when nothing is in progress and the window is on screen; the prompt
                // never touches capture. Nothing downloads or installs until "Update now".
                if pending_notice.is_some()
                    && !checking_update
                    && staged_update.is_none()
                    && !saving.load(Ordering::SeqCst)
                    && !crate::editor::is_open()
                    && !recording
                    && !recordfinishing
                    && !recovering
                    && IsWindowVisible(hwnd) != 0
                    && IsIconic(hwnd) == 0
                    && GetForegroundWindow() == hwnd
                {
                    let v = pending_notice.take().unwrap();
                    writeln!(log.lock().unwrap(), "UPDATE notice shown for {v}").ok();
                    let ask = crate::update_prompt::ask(hwnd, &v);
                    if ask == crate::update_prompt::UPDATE_NOW {
                        checking_update = true;
                        install_when_staged = true;
                        set(updatebutton, "Checking / downloading update...");
                        let t = update_tx.clone();
                        let progress = update_progress_tx.clone();
                        writeln!(log.lock().unwrap(), "UPDATE started from notice").ok();
                        thread::spawn(move || {
                            t.send(crate::updater::stage(progress)).ok();
                        });
                    } else {
                        let r = if ask == crate::update_prompt::REMIND_LATER {
                            crate::updater::remind_later(&root, &v)
                        } else {
                            crate::updater::skip_version(&root, &v)
                        };
                        if let Err(e) = r {
                            writeln!(log.lock().unwrap(), "UPDATE notice choice not saved: {e}").ok();
                        }
                    }
                }
                while let Ok(progress) = update_progress_rx.try_recv() {
                    set(updatebutton, &progress);
                    set(result, &progress);
                    writeln!(log.lock().unwrap(), "UPDATE {progress}").ok();
                }
                if let Ok(outcome) = update_rx.try_recv() {
                    checking_update = false;
                    if outcome.is_err() {
                        install_when_staged = false;
                    }
                    match outcome {
                        Ok(Some(staged)) => {
                            writeln!(log.lock().unwrap(), "UPDATE staged {}", staged.version).ok();
                            set(
                                updatebutton,
                                &format!("Install {} and restart", staged.version),
                            );
                            set(
                                result,
                                "Verified update staged. Click Install when you're ready.",
                            );
                            if install_when_staged {
                                install_when_staged = false;
                                if saving.load(Ordering::SeqCst) || crate::editor::is_open() || recording || recordfinishing || recovering {
                                    set(result, "Verified update staged. Capture is busy (saving, recording, recovering or the workbench is open). Nothing was restarted. Click Install when you are ready; you will be warned again.");
                                } else {
                                    match crate::updater::apply(&staged, &root) {
                                        Ok(()) => {
                                            PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, 3, 0);
                                        }
                                        Err(e) => set(result, &format!("Update could not start: {e}")),
                                    }
                                }
                            }
                            staged_update = Some(staged);
                        }
                        Ok(None) => {
                            writeln!(log.lock().unwrap(), "UPDATE current version").ok();
                            set(updatebutton, "Check / download update");
                            set(result, "You're on the latest published build.");
                        }
                        Err(e) => {
                            set(updatebutton, "Retry update check");
                            set(
                                result,
                                "Update check failed. Current install untouched. See Diagnostics.",
                            );
                            writeln!(log.lock().unwrap(), "UPDATE: {e}").ok();
                        }
                    }
                }
                let editor_saves = crate::editor::take_saved();
                if !editor_saves.is_empty() {
                    SetPropW(hwnd, wide("library-offset").as_ptr(), 0usize as _);
                    refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                    crate::thumbs::request(editor_saves, thumbtx.clone());
                    writeln!(log.lock().unwrap(), "LIBRARY refreshed after editor export").ok();
                }
                while let Ok((path, bitmap)) = thumbrx.try_recv() {
                    crate::ui::store_thumb(path, bitmap);
                    for row in &recentrows {
                        InvalidateRect(*row, std::ptr::null(), 0);
                    }
                }
                while let Ok(out) = rx.try_recv() {
                    match out {
                        Ok(p) => {
                            SetPropW(hwnd, wide("library-offset").as_ptr(), 0usize as _);
                            refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                            crate::thumbs::request(vec![p.clone()], thumbtx.clone());
                            set(
                                result,
                                &format!(
                                    "Saved {}\nOpen Clips to play or share it.",
                                    p.file_name().unwrap().to_string_lossy()
                                ),
                            );
                            if sound_on {
                                crate::feedback::ding();
                            }
                            if toast_on {
                                crate::feedback::show(hwnd);
                            }
                        }
                        Err(e) => {
                            set(result, "Save failed. Open Diagnostics for details.");
                            writeln!(log.lock().unwrap(), "SAVE ERROR: {e}").ok();
                        }
                    }
                }
                while let Ok(outcome) = recordrx.try_recv() {
                    recording = false;
                    recordfinishing = false;
                    recovering = false;
                    refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                    EnableWindow(recordbutton, 1);
                    set(recordbutton, "Start recording");
                    match outcome {
                        Ok(p) => {
                            set(
                                result,
                                &format!(
                                    "Recording saved: {}",
                                    p.file_name().unwrap_or_default().to_string_lossy()
                                ),
                            );
                            SetPropW(hwnd, wide("library-offset").as_ptr(), 0usize as _);
                            refresh_recent(&dir, &recentrows, &mut recentpaths, !settings_open);
                            crate::thumbs::request(vec![p], thumbtx.clone());
                        }
                        Err(e) => {
                            writeln!(log.lock().unwrap(), "RECORDING ERROR: {e}").ok();
                            set(result,"Recording needs attention. Open Diagnostics. Raw files kept if recoverable.");
                        }
                    }
                    // Same meaning of "pending" as the refresh path: a recording in progress is never recoverable.
                    let recover_now = crate::recording::pending(&dir) && !recording;
                    crate::nav::set_recover(recover_now);
                    ShowWindow(
                        recoverbutton,
                        if crate::nav::recover_visible(
                            recover_now,
                            settings_open,
                            crate::polished::sidebar()
                                || crate::nav::compact_page(layout, crate::polished::enabled(), false).is_some(),
                            crate::polished::nav_page(),
                        ) {
                            SW_SHOW
                        } else {
                            SW_HIDE
                        },
                    );
                }
                if last.elapsed() >= Duration::from_millis(500) {
                    last = Instant::now();
                    if paused {
                        continue;
                    }
                    {
                        let current = audio_status.lock().unwrap().clone();
                        if current != last_audio_status {
                            writeln!(log.lock().unwrap(), "AUDIO: {current}").ok();
                            last_audio_status = current;
                        }
                    }
                    set(
                        audiolabel,
                        &format!(
                            "{}\n{}\n{}",
                            audio_status.lock().unwrap(),
                            mic_status.lock().unwrap(),
                            crate::comms::status().lock().unwrap()
                        ),
                    );
                    let (has_frames, error, count, stale) = {
                        let r = ring.lock().unwrap();
                        (
                            !r.parts.is_empty(),
                            r.error.clone(),
                            r.parts.len(),
                            r.parts
                                .back()
                                .is_some_and(|p| p.at.elapsed() > Duration::from_secs(5)),
                        )
                    };
                    if recording && (error.is_some() || stale) {
                        crate::recording::stop(&recordtap);
                        recording = false;
                        recordfinishing = true;
                        EnableWindow(recordbutton, 0);
                        set(recordbutton, "Finishing recording...");
                        set(result, "Capture interrupted. Saving the recorded portion.");
                    }
                    if error.is_some() && !failed {
                        writeln!(
                            log.lock().unwrap(),
                            "Capture mode compatibility={compat}: {error:?}"
                        )
                        .ok();
                        if !compat && !ready {
                            let _ = child.kill();
                            let _ = child.wait();
                            compat = true;
                            set(pathbutton, "CPU fallback");
                            // Keep the user's chosen path in Settings; Details reports actual fallback.
                            ring = Arc::new(Mutex::new(Ring::with_seconds(buffer_seconds)));
                            mic_status = Arc::new(Mutex::new(
                                if mic_on { "Starting mic..." } else { "Mic OFF" }.into(),
                            ));
                            audio_status =
                                Arc::new(Mutex::new(String::from("Starting fallback audio...")));
                            writeln!(log.lock().unwrap(), "AUTO FALLBACK compatibility=true").ok();
                            child = start(
                                &exe,
                                index,
                                true,
                                audio_status.clone(),
                                gain.clone(),
                                log.clone(),
                                audio_on,
                                capture_width,
                                capture_fps,
                                mic_on,
                                mic_gain.clone(),
                                mic_status.clone(),
                            )
                            .map_err(|e| e.to_string())?;
                            if AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
                                let _ = child.kill();
                                return Err("Cannot manage fallback capture.".into());
                            }
                            attach_reader(&mut child, ring.clone(), log.clone(), recordtap.clone());
                            began = Instant::now();
                            set(status, "Restarting...");
                            set(detail,"Fast GPU path failed. Hardware encoding stays on; color conversion uses CPU.");
                            continue;
                        }
                        failed = true;
                        EnableWindow(restartbutton, 1);
                        EnableWindow(savebutton, 0);
                        set(status, "Capture stopped");
                        set(
                            result,
                            "Capture stopped. Saved clips are safe; Restart to resume.",
                        );
                        set(detail,"Press Restart capture after sleep, locking Windows or display changes.\nIf it still fails, open Diagnostics and send the error text.");
                    }
                    if has_frames && !failed {
                        if !ready {
                            // Keep an unresolved settings-import notice visible instead of replacing it.
                            match lock_note.take() {
                                Some(n) if startup_blocked && !import_resolved => {
                                    set(result, &n);
                                    lock_note = Some(n);
                                }
                                Some(n) => set(result, &n),
                                None => set(result, "Replay ready. Save a moment whenever you want."),
                            }
                        }
                        ready = true;
                        EnableWindow(restartbutton, 1);
                        EnableWindow(savebutton, 1);
                        set(
                            status,
                            if stale {
                                "Capture stalled"
                            } else if count >= buffer_seconds as usize {
                                if recording {
                                    "Recording to file"
                                } else {
                                    "Replay active"
                                }
                            } else {
                                if recording {
                                    "Recording to file"
                                } else {
                                    "Replay active"
                                }
                            },
                        );
                        set(
                            detail,
                            &format!(
                                "{} | H.264 / {} FPS target\n{} / {buffer_seconds} chunks buffered",
                                if compat {
                                    "CPU conversion"
                                } else {
                                    "GPU conversion"
                                },
                                capture_fps,
                                count
                            ),
                        );
                    }
                    let a = audio_status.lock().unwrap().clone();
                    let mi = mic_status.lock().unwrap().clone();
                    let audio_problem = audio_on
                        && (a.contains("unavailable")
                            || a.contains("failed")
                            || a.contains("error"));
                    let mic_problem = mic_on
                        && (mi.contains("unavailable")
                            || mi.contains("failed")
                            || mi.contains("error"));
                    set(
                        warning,
                        if failed || stale {
                            "Capture needs attention. Restart or open Diagnostics."
                        } else if audio_problem
                            || mic_problem
                            || crate::comms::status()
                                .lock()
                                .unwrap()
                                .contains("unavailable")
                            || crate::comms::status().lock().unwrap().contains("error")
                            || crate::comms::status().lock().unwrap().contains("failed")
                        {
                            "Audio needs attention. Open Details for the error."
                        } else {
                            ""
                        },
                    );
                    crate::replay_design::attention(hwnd);
                    EnableWindow(
                        recoverbutton,
                        if recording || recordfinishing || recovering {
                            0
                        } else {
                            1
                        },
                    );
                    EnableWindow(
                        capturebutton,
                        if recording || recordfinishing { 0 } else { 1 },
                    );
                    EnableWindow(
                        restartbutton,
                        if recording || recordfinishing { 0 } else { 1 },
                    );
                    if recording {
                        let elapsed = recordbegan.elapsed().as_secs();
                        set(
                            recordbutton,
                            &format!("Stop recording  {}:{:02}", elapsed / 60, elapsed % 60),
                        );
                    }
                    crate::replay_design::buffer(
                        bufferbar,
                        count.min(buffer_seconds as usize),
                        buffer_seconds as usize,
                    );
                    set(
                        encoderlabel,
                        &format!(
                            "{} / H.264 / {} wide / {} FPS",
                            if compat {
                                "CPU conversion"
                            } else {
                                "GPU conversion"
                            },
                            capture_width,
                            capture_fps
                        ),
                    );
                    let full_hint = format!("Last {buffer_seconds}s ready");
                    let filling = if GetPropW(hwnd, wide("display-layout").as_ptr()) as usize == 2 {
                        format!(
                            "Filling / {} of {buffer_seconds}s",
                            count.min(buffer_seconds as usize)
                        )
                    } else {
                        format!(
                            "Filling buffer / {} of {buffer_seconds} seconds",
                            count.min(buffer_seconds as usize)
                        )
                    };
                    set(
                        simplehint,
                        if failed {
                            "Your saved clips are safe"
                        } else if paused {
                            "Replay is paused"
                        } else if count >= buffer_seconds as usize {
                            &full_hint
                        } else {
                            &filling
                        },
                    );
                    if !ready && !failed && began.elapsed() > Duration::from_secs(15) {
                        set(status, "Waiting for capture...");
                        set(
                            detail,
                            "No video frames yet. Open Diagnostics or restart capture.",
                        );
                    }
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
}
fn startup_note(message: &str) {
    use std::io::Write;
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join("startup-diagnostics.txt"))
            {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                writeln!(
                    f,
                    "{stamp} ZimClip {}: {message}",
                    env!("CARGO_PKG_VERSION")
                )
                .ok();
            }
        }
    }
}
fn main() {
    startup_note("Process entered");
    std::panic::set_hook(Box::new(|info| {
        startup_note(&format!("PANIC: {info}"));
    }));
    #[cfg(windows)]
    if let Err(e) = app::run() {
        startup_note(&format!("Startup/runtime error: {e}"));
        unsafe {
            let text: Vec<u16> = format!("ZimClip: {e}")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let title: Vec<u16> = "ZimClip".encode_utf16().chain(Some(0)).collect();
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                0x10,
            );
        }
    }
}
