use std::path::Path;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureSettings {
    pub monitor: u32,
    pub compatibility: bool,
    pub width: u32,
    pub fps: u32,
    pub system_audio: bool,
    pub microphone: bool,
    pub system_gain: i32,
    pub microphone_gain: i32,
}
impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            monitor: 0,
            compatibility: true,
            width: 1280,
            fps: 30,
            system_audio: true,
            microphone: false,
            system_gain: 0,
            microphone_gain: 0,
        }
    }
}
impl CaptureSettings {
    pub fn parse(s: &str) -> Self {
        let mut v = Self::default();
        for line in s.lines() {
            let Some((k, n)) = line.trim().split_once('=') else {
                continue;
            };
            let n = n.trim();
            match k.trim() {
                "monitor" => {
                    if let Ok(x) = n.parse::<u32>() {
                        if x < 64 {
                            v.monitor = x
                        }
                    }
                }
                "compatibility" => match n {
                    "0" => v.compatibility = false,
                    "1" => v.compatibility = true,
                    _ => {}
                },
                "width" => {
                    if let Ok(x) = n.parse() {
                        if [854, 1280, 1920].contains(&x) {
                            v.width = x
                        }
                    }
                }
                "fps" => {
                    if let Ok(x) = n.parse() {
                        if [30, 60].contains(&x) {
                            v.fps = x
                        }
                    }
                }
                "system_audio" => match n {
                    "0" => v.system_audio = false,
                    "1" => v.system_audio = true,
                    _ => {}
                },
                "microphone" => match n {
                    "0" => v.microphone = false,
                    "1" => v.microphone = true,
                    _ => {}
                },
                "system_gain" => {
                    if let Ok(x) = n.parse::<i32>() {
                        if (0..=24).contains(&x) {
                            v.system_gain = x
                        }
                    }
                }
                "microphone_gain" => {
                    if let Ok(x) = n.parse::<i32>() {
                        if (0..=24).contains(&x) {
                            v.microphone_gain = x
                        }
                    }
                }
                _ => {}
            }
        }
        v
    }
    pub fn load(root: &Path) -> Self {
        Self::parse(&std::fs::read_to_string(root.join("settings.ini")).unwrap_or_default())
    }
    pub fn text(&self) -> String {
        format!("monitor={}\ncompatibility={}\nwidth={}\nfps={}\nsystem_audio={}\nmicrophone={}\nsystem_gain={}\nmicrophone_gain={}\n",self.monitor,self.compatibility as u8,self.width,self.fps,self.system_audio as u8,self.microphone as u8,self.system_gain,self.microphone_gain)
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        use std::io::Write;
        let dst = root.join("settings.ini");
        let tmp = root.join("settings.ini.tmp");
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(self.text().as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        drop(f);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let from: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = dst.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                        | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(windows))]
        std::fs::rename(tmp, dst).map_err(|e| e.to_string())?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let v = CaptureSettings {
            monitor: 2,
            compatibility: false,
            width: 1920,
            fps: 60,
            system_audio: false,
            microphone: true,
            system_gain: 18,
            microphone_gain: 12,
        };
        assert_eq!(CaptureSettings::parse(&v.text()), v)
    }
    #[test]
    fn migrate_old() {
        let v = CaptureSettings::parse("monitor=1\ncompatibility=0\n");
        assert_eq!(v.monitor, 1);
        assert!(!v.compatibility);
        assert_eq!(v.width, 1280);
        assert!(!v.microphone)
    }
    #[test]
    fn reject_bad() {
        assert_eq!(
            CaptureSettings::parse(
                "width=9999\nfps=-2\nmicrophone=hi\nsystem_gain=999\nmonitor=999\n"
            ),
            CaptureSettings::default()
        )
    }
    #[test]
    fn replaces_existing() {
        let d = std::env::temp_dir().join(format!("zim-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let mut v = CaptureSettings::default();
        v.save(&d).unwrap();
        v.width = 1920;
        v.save(&d).unwrap();
        assert_eq!(CaptureSettings::load(&d), v);
        std::fs::remove_dir_all(d).unwrap();
    }
}
