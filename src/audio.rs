use std::{
    collections::VecDeque,
    io::Write,
    process::ChildStdin,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use wasapi::*;
pub type Status = Arc<Mutex<String>>;
pub fn spawn(
    mut pipe: ChildStdin,
    status: Status,
    gain: Arc<AtomicI32>,
    log: Arc<Mutex<std::fs::File>>,
    mic_on: bool,
    mic_gain: Arc<AtomicI32>,
    mic_status: Status,
    system_on: bool,
) {
    thread::spawn(move || {
        if let Err(e) = capture(
            &mut pipe,
            &status,
            gain,
            log.clone(),
            mic_on,
            mic_gain,
            mic_status.clone(),
            system_on,
        ) {
            writeln!(log.lock().unwrap(), "AUDIO error: {e}").ok();
            *status.lock().unwrap() = format!("Audio unavailable: {e}");
            *mic_status.lock().unwrap() =
                "Mic stopped with audio pipeline. See Diagnostics.".into();
        }
    });
}
fn capture(
    pipe: &mut ChildStdin,
    status: &Status,
    gain: Arc<AtomicI32>,
    log: Arc<Mutex<std::fs::File>>,
    mic_on: bool,
    mic_gain: Arc<AtomicI32>,
    mic_status: Status,
    system_on: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    initialize_mta().ok()?;
    let isolated = crate::comms::enabled();
    let mut system = if system_on && !isolated {
        match Source::open(false, None) {
            Ok(s) => Some(s),
            Err(e) => {
                *status.lock().unwrap() = format!("System unavailable: {e}");
                None
            }
        }
    } else {
        None
    };
    let mut sessions: Vec<(u32, bool, Source)> = Vec::new();
    let mut scan = Instant::now() - Duration::from_secs(3);
    let mut last_roots: Vec<(u32, bool)> = Vec::new();
    let mut root_lines = 0u32;
    let mut mic = if mic_on {
        match Source::open(true, None) {
            Ok(m) => Some(m),
            Err(e) => {
                writeln!(log.lock().unwrap(), "MIC unavailable, track silent: {e}").ok();
                *mic_status.lock().unwrap() = format!("Mic unavailable: {e}");
                None
            }
        }
    } else {
        *mic_status.lock().unwrap() = "Mic OFF".into();
        None
    };
    for (label, source) in [("System", &system), ("Mic", &mic)] {
        if let Some(s) = source {
            writeln!(
                log.lock().unwrap(),
                "{label} independent capture: {}",
                s.name
            )
            .ok();
        }
    }
    let mut sys = vec![0u8; 1920];
    let mut microphone = vec![0u8; 1920];
    let mut comms = vec![0u8; 1920];
    for (source, report) in [(&mut system, status), (&mut mic, &mic_status)] {
        if let Some(s) = source {
            if let Err(e) = s.client.start_stream() {
                *report.lock().unwrap() = format!("Capture startup failed: {e}");
                *source = None;
            }
        }
    }
    let mut next = Instant::now() + Duration::from_millis(30);
    let mut packed = Vec::with_capacity(7680);
    let mut endpoint_check = Instant::now();
    loop {
        for (source, report) in [(&mut system, status), (&mut mic, &mic_status)] {
            if let Some(s) = source {
                if let Err(e) = s.poll() {
                    *report.lock().unwrap() = format!("Capture error: {e}");
                    *source = None;
                }
            }
        }
        if endpoint_check.elapsed() >= Duration::from_secs(2) {
            endpoint_check = Instant::now();
            for (microphone, on, source, report) in [
                (false, system_on && !isolated, &mut system, status),
                (true, mic_on, &mut mic, &mic_status),
            ] {
                if !on {
                    continue;
                }
                let selected_id =
                    crate::audio_devices::device(microphone).and_then(|d| Ok(d.get_id()?));
                if source
                    .as_ref()
                    .is_some_and(|s| selected_id.as_ref().ok() != s.endpoint_id.as_ref())
                {
                    *source = None;
                }
                if source.is_none() {
                    match Source::open(microphone, None).and_then(|s| {
                        s.client.start_stream()?;
                        Ok(s)
                    }) {
                        Ok(s) => {
                            *report.lock().unwrap() = format!("Capture reconnected: {}", s.name);
                            *source = Some(s)
                        }
                        Err(e) => {
                            *report.lock().unwrap() =
                                format!("Selected device unavailable; retrying: {e}")
                        }
                    }
                }
            }
        }
        if isolated && scan.elapsed() >= Duration::from_secs(2) {
            scan = Instant::now();
            match crate::comms::audio_roots() {
                Ok(ids) => {
                    let mut key = ids.clone();
                    key.sort();
                    if key != last_roots && root_lines < 200 {
                        root_lines += 1;
                        writeln!(
                            log.lock().unwrap(),
                            "ROUTING roots {}",
                            crate::comms::describe_roots(&key)
                        )
                        .ok();
                    }
                    last_roots = key;
                    sessions.retain(|(pid, voice, _)| ids.contains(&(*pid, *voice)));
                    let mut failures = 0;
                    for (pid, voice) in ids {
                        if (!voice && !system_on)
                            || sessions.iter().any(|s| s.0 == pid && s.1 == voice)
                        {
                            continue;
                        }
                        match Source::open(false, Some((pid, true))).and_then(|s| {
                            s.client.start_stream()?;
                            Ok(s)
                        }) {
                            Ok(s) => sessions.push((pid, voice, s)),
                            Err(e) => {
                                failures += 1;
                                writeln!(
                                    log.lock().unwrap(),
                                    "ROUTING PID={pid} voice={voice} unavailable: {e}"
                                )
                                .ok();
                            }
                        }
                    }
                    *crate::comms::status().lock().unwrap() = if failures > 0 {
                        format!("Voice routing: {failures} source(s) unavailable; retrying")
                    } else {
                        format!(
                            "Voice chat: {} audio trees / {}",
                            sessions.iter().filter(|s| s.1).count(),
                            crate::comms::configured()
                        )
                    };
                    if system_on {
                        *status.lock().unwrap() = format!(
                            "System: {} non-voice audio trees{}",
                            sessions.iter().filter(|s| !s.1).count(),
                            if failures > 0 {
                                "; routing incomplete"
                            } else {
                                ""
                            }
                        );
                    }
                }
                Err(e) => {
                    sessions.clear();
                    *crate::comms::status().lock().unwrap() = format!("Routing unavailable: {e}");
                    *status.lock().unwrap() = "System silent: isolated routing unavailable".into();
                }
            }
        }
        sessions.retain_mut(|(pid, voice, s)| match s.poll() {
            Ok(()) => true,
            Err(e) => {
                writeln!(
                    log.lock().unwrap(),
                    "ROUTING PID={pid} voice={voice} stopped: {e}"
                )
                .ok();
                *crate::comms::status().lock().unwrap() =
                    "Voice routing source stopped; retrying".into();
                false
            }
        });
        if Instant::now() >= next {
            sys.fill(0);
            microphone.fill(0);
            comms.fill(0);
            let mut part = [0u8; 1920];
            for (_, voice, s) in &mut sessions {
                part.fill(0);
                s.read(
                    &mut part,
                    if *voice {
                        0
                    } else {
                        gain.load(Ordering::Relaxed)
                    },
                );
                crate::levels::mix_pcm(if *voice { &mut comms } else { &mut sys }, &part, 0);
            }
            if let Some(s) = system.as_mut() {
                s.read(&mut sys, gain.load(Ordering::Relaxed));
                s.report(status, &log, gain.load(Ordering::Relaxed));
            }
            if let Some(s) = mic.as_mut() {
                s.read(&mut microphone, mic_gain.load(Ordering::Relaxed));
                s.report(&mic_status, &log, mic_gain.load(Ordering::Relaxed));
            }
            crate::audio_tracks::pack(&sys, &microphone, &comms, &mut packed);
            if pipe.write_all(&packed).is_err() {
                break;
            }
            next += Duration::from_millis(10);
            if next.elapsed() > Duration::from_millis(250) {
                next = Instant::now() + Duration::from_millis(10);
                if let Some(s) = system.as_mut() {
                    s.queue.clear();
                }
                if let Some(s) = mic.as_mut() {
                    s.queue.clear();
                }
                for (_, _, s) in &mut sessions {
                    s.queue.clear();
                }
            }
        } else {
            thread::sleep(Duration::from_millis(2));
        }
    }
    Ok(())
}
struct Source {
    client: AudioClient,
    reader: AudioCaptureClient,
    scratch: Vec<u8>,
    queue: VecDeque<u8>,
    name: String,
    meter: crate::levels::Meter,
    last_meter: Instant,
    gaps: u64,
    process_handle: Option<windows_sys::Win32::Foundation::HANDLE>,
    endpoint_id: Option<String>,
}
impl Source {
    fn open(
        microphone: bool,
        process: Option<(u32, bool)>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut endpoint_id = None;
        let (mut client, name) = if let Some((pid, include)) = process {
            (
                AudioClient::new_application_loopback_client(pid, include)?,
                format!("Process {pid}"),
            )
        } else {
            let device = crate::audio_devices::device(microphone)?;
            endpoint_id = Some(device.get_id()?);
            (device.get_iaudioclient()?, device.get_friendlyname()?)
        };
        let format = WaveFormat::new(16, 16, SampleType::Int, 48000, 2, None);
        client.initialize_client(
            &format,
            Direction::Capture,
            StreamMode::PollingShared {
                autoconvert: true,
                buffer_duration_hns: 1_000_000,
            },
            None,
        )?;
        let reader = client.get_audiocaptureclient()?;
        // Process-loopback GetBufferSize can return a meaningless huge value.
        let frames = if process.is_some() {
            48000
        } else {
            (client.get_buffer_size()? as usize).clamp(4800, 48000)
        };
        let process_handle = if let Some((pid, _)) = process {
            let h = unsafe {
                windows_sys::Win32::System::Threading::OpenProcess(
                    windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    pid,
                )
            };
            if h.is_null() {
                return Err(
                    "Selected process ended or cannot be checked. Choose another app.".into(),
                );
            }
            Some(h)
        } else {
            None
        };
        Ok(Self {
            client,
            reader,
            scratch: vec![0; frames * 4],
            queue: VecDeque::new(),
            name,
            meter: crate::levels::Meter::new(),
            last_meter: Instant::now(),
            gaps: 0,
            process_handle,
            endpoint_id,
        })
    }
    fn poll(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(h) = self.process_handle {
            let mut code = 0;
            let ok =
                unsafe { windows_sys::Win32::System::Threading::GetExitCodeProcess(h, &mut code) };
            if ok == 0 || code != 259 {
                return Err(
                    "Selected app ended. Reselect Comms app to resume isolated audio.".into(),
                );
            }
        }
        while self.reader.get_next_packet_size()?.unwrap_or(0) > 0 {
            let (frames, info) = self.reader.read_from_device(&mut self.scratch)?;
            if info.flags.data_discontinuity {
                self.gaps += 1;
            }
            if info.flags.silent {
                self.queue
                    .extend(std::iter::repeat_n(0, frames as usize * 4));
            } else {
                self.queue.extend(&self.scratch[..frames as usize * 4]);
            }
        }
        if self.queue.len() > 48000 * 4 {
            self.queue.clear();
            self.gaps += 1;
        }
        Ok(())
    }
    fn read(&mut self, out: &mut [u8], gain: i32) {
        for x in out.iter_mut() {
            *x = self.queue.pop_front().unwrap_or(0);
        }
        self.meter.process(out, gain);
    }
    fn report(&mut self, status: &Status, log: &Arc<Mutex<std::fs::File>>, gain: i32) {
        if self.last_meter.elapsed() > Duration::from_secs(2) {
            let (peak, rms) = self.meter.db();
            *status.lock().unwrap() = format!("{} | {peak:.1}dB peak | +{gain}dB", self.name);
            writeln!(
                log.lock().unwrap(),
                "AUDIO source={} peak={peak:.2} RMS={rms:.2} boost={gain} gaps={}",
                self.name,
                self.gaps
            )
            .ok();
            self.meter = crate::levels::Meter::new();
            self.last_meter = Instant::now();
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        let _ = self.client.stop_stream();
        if let Some(h) = self.process_handle {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(h);
            }
        }
    }
}
