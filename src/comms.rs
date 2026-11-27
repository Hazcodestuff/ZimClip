use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Mutex, OnceLock},
};
fn names() -> &'static Mutex<Vec<String>> {
    static N: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    N.get_or_init(Default::default)
}
pub fn normalize(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for n in s.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let n = n.to_lowercase();
        let n = n.strip_suffix(".exe").unwrap_or(&n);
        if n.is_empty()
            || n == "."
            || n == ".."
            || n.len() > 80
            || n.chars()
                .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_' | '.'))
        {
            return Err("Use app file names separated by commas, without paths.".into());
        }
        if !out.contains(&n.to_string()) {
            out.push(n.to_string())
        }
    }
    if out.len() > 24 {
        return Err("Use up to24voice-chat app names.".into());
    }
    Ok(out)
}
pub fn load(root: &Path) {
    *names().lock().unwrap() = std::fs::read_to_string(root.join("voice-chat-apps.txt"))
        .ok()
        .and_then(|s| normalize(&s).ok())
        .unwrap_or_default()
}
pub fn configured() -> String {
    names().lock().unwrap().join(", ")
}
pub fn enabled() -> bool {
    !names().lock().unwrap().is_empty()
}
pub fn save(root: &Path, s: &str) -> Result<(), String> {
    let n = normalize(s)?;
    let path = root.join("voice-chat-apps.txt");
    let temp = root.join("voice-chat-apps.txt.tmp");
    std::fs::write(&temp, n.join(", ")).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let a: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())?;
    *names().lock().unwrap() = n;
    Ok(())
}
#[derive(Clone, Debug)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
}
pub fn is_voice(pid: u32, processes: &BTreeMap<u32, Process>, apps: &[String]) -> bool {
    let mut next = pid;
    let mut seen = BTreeSet::new();
    while seen.insert(next) {
        let Some(p) = processes.get(&next) else { break };
        if apps
            .iter()
            .any(|n| p.name.trim_end_matches(".exe").eq_ignore_ascii_case(n))
        {
            return true;
        }
        next = p.parent;
    }
    false
}
pub fn roots(pids: &[u32], processes: &BTreeMap<u32, Process>) -> Vec<u32> {
    let set: BTreeSet<_> = pids.iter().copied().filter(|p| *p > 0).collect();
    set.iter()
        .copied()
        .filter(|pid| {
            let mut n = processes.get(pid).map(|p| p.parent).unwrap_or(0);
            let mut seen = BTreeSet::new();
            while n > 0 && seen.insert(n) {
                if set.contains(&n) {
                    return false;
                }
                n = processes.get(&n).map(|p| p.parent).unwrap_or(0)
            }
            true
        })
        .collect()
}
#[cfg(windows)]
pub fn processes() -> BTreeMap<u32, Process> {
    use windows_sys::Win32::{Foundation::*, System::Diagnostics::ToolHelp::*};
    let mut out = BTreeMap::new();
    unsafe {
        let h = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if h == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut p: PROCESSENTRY32W = std::mem::zeroed();
        p.dwSize = std::mem::size_of_val(&p) as u32;
        if Process32FirstW(h, &mut p) != 0 {
            loop {
                let n = p
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(p.szExeFile.len());
                out.insert(
                    p.th32ProcessID,
                    Process {
                        pid: p.th32ProcessID,
                        parent: p.th32ParentProcessID,
                        name: String::from_utf16_lossy(&p.szExeFile[..n]).to_lowercase(),
                    },
                );
                if Process32NextW(h, &mut p) == 0 {
                    break;
                }
            }
        }
        CloseHandle(h);
    }
    out
}
#[cfg(windows)]
pub fn audio_roots() -> Result<Vec<(u32, bool)>, String> {
    use wasapi::*;
    let processes = processes();
    let apps = names().lock().unwrap().clone();
    let devices = DeviceEnumerator::new()
        .and_then(|e| e.get_device_collection(Direction::Render))
        .map_err(|e| e.to_string())?;
    let mut pids = Vec::new();
    let mut enumerated = 0;
    for d in &devices {
        if let Ok(d) = d {
            if let Ok(e) = d
                .get_iaudiosessionmanager()
                .and_then(|m| m.get_audiosessionenumerator())
            {
                enumerated += 1;
                for i in 0..e.get_count().unwrap_or(0) {
                    if let Ok(p) = e.get_session(i).and_then(|s| s.get_process_id()) {
                        if p > 0 && !pids.contains(&p) {
                            pids.push(p)
                        }
                    }
                }
            }
        }
    }
    if enumerated == 0 {
        return Err("No render audio sessions could be inspected".into());
    }
    let result = roots(&pids, &processes);
    for pid in &pids {
        let voice = is_voice(*pid, &processes, &apps);
        let mut next = *pid;
        let mut seen = BTreeSet::new();
        while seen.insert(next) {
            if result.contains(&next) && is_voice(next, &processes, &apps) != voice {
                return Err("Overlapping app trees cross System and Voice chat; close the parent audio app and restart replay".into());
            }
            let Some(p) = processes.get(&next) else { break };
            next = p.parent;
        }
    }
    Ok(result
        .into_iter()
        .map(|p| (p, is_voice(p, &processes, &apps)))
        .collect())
}
pub fn describe_roots(roots: &[(u32, bool)]) -> String {
    #[cfg(windows)]
    let procs = processes();
    #[cfg(not(windows))]
    let procs: BTreeMap<u32, Process> = BTreeMap::new();
    let list: Vec<String> = roots
        .iter()
        .map(|(pid, voice)| {
            let n = procs.get(pid).map(|p| p.name.as_str()).unwrap_or("?");
            format!("{pid}:{n}:{}", if *voice { "voice" } else { "system" })
        })
        .collect();
    format!("[{}] listed=[{}]", list.join(" "), configured())
}
pub fn status() -> &'static Mutex<String> {
    static S: OnceLock<Mutex<String>> = OnceLock::new();
    S.get_or_init(|| Mutex::new("Voice chat OFF".into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_names() {
        assert_eq!(
            normalize(" Discord.exe,discord, Teams ").unwrap(),
            vec!["discord", "teams"]
        );
        assert!(normalize("C:\\Discord").is_err());
        assert!(normalize(".exe").is_err())
    }
    #[test]
    fn saved_names_reload() {
        let dir = std::env::temp_dir().join(format!("zimclip-comms-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        save(&dir, "Discord.EXE, Teams").unwrap();
        *names().lock().unwrap() = Vec::new();
        load(&dir);
        assert_eq!(configured(), "discord, teams");
        assert!(enabled());
        assert!(save(&dir, "C:\\bad").is_err());
        assert_eq!(configured(), "discord, teams");
        save(&dir, "").unwrap();
        load(&dir);
        assert!(!enabled());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn process_trees() {
        let p = [
            Process {
                pid: 10,
                parent: 1,
                name: "discord.exe".into(),
            },
            Process {
                pid: 11,
                parent: 10,
                name: "discord.exe".into(),
            },
            Process {
                pid: 12,
                parent: 11,
                name: "helper.exe".into(),
            },
            Process {
                pid: 20,
                parent: 1,
                name: "game.exe".into(),
            },
        ]
        .into_iter()
        .map(|p| (p.pid, p))
        .collect();
        assert!(is_voice(12, &p, &["discord".into()]));
        assert!(!is_voice(20, &p, &["discord".into()]));
        assert_eq!(roots(&[10, 11, 12, 20], &p), vec![10, 20]);
        assert_eq!(roots(&[11, 12, 20], &p), vec![11, 20]);
    }
}
