use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
const MANIFEST: &str = "https://hadir-reminders.3jk1zs.workers.dev/dl/zimclip-latest.json";
pub struct Staged {
    pub version: String,
    pub folder: PathBuf,
}
pub fn stage(progress: std::sync::mpsc::Sender<String>) -> Result<Option<Staged>, String> {
    use std::io::{BufRead, BufReader, Read};
    let script = r#"
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
Write-Output 'STATUS:Connecting to update server...'
[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12
$m=Invoke-RestMethod -Uri $env:ZC_MANIFEST -MaximumRedirection 0 -TimeoutSec 30
if($m.version -notmatch '^\d+\.\d+(\.\d+)?$'){throw 'Invalid version'}
$versionText=[string]$m.version;if(($versionText.Split('.')).Count -eq 2){$versionText+='.0'}
if($env:ZC_PRE -eq '1'){$stale=([version]$versionText -lt [version]$env:ZC_VERSION)}else{$stale=([version]$versionText -le [version]$env:ZC_VERSION)}
if($stale){Write-Output 'CURRENT';exit 0}
$u=[uri]$m.url
if($u.Scheme -ne 'https' -or $u.Port -ne 443 -or $u.UserInfo -ne '' -or $u.Query -ne '' -or $u.Fragment -ne '' -or $u.Host -ne 'hadir-reminders.3jk1zs.workers.dev' -or $u.AbsolutePath -notmatch '^/dl/zimclip-[0-9.]+\.zip$'){throw 'Unapproved update URL'}
if($m.sha256 -notmatch '^[a-fA-F0-9]{64}$' -or [long]$m.bytes -lt 100000 -or [long]$m.bytes -gt 100000000){throw 'Invalid download metadata'}
$d=Join-Path ([IO.Path]::GetTempPath()) ('ZimClip-update-'+[guid]::NewGuid().ToString('N'));New-Item -ItemType Directory $d|Out-Null
try{
 $zip=Join-Path $d 'update.zip'
 Write-Output 'STATUS:Downloading update... 0%'
 $request=[Net.HttpWebRequest]::Create($u.AbsoluteUri);$request.AllowAutoRedirect=$false;$request.Timeout=30000;$request.ReadWriteTimeout=30000
 $response=$request.GetResponse();$stream=$response.GetResponseStream();$file=[IO.File]::Create($zip)
 try{$buffer=New-Object byte[] 65536;$received=0L;$last=-1
 while(($read=$stream.Read($buffer,0,$buffer.Length)) -gt 0){$file.Write($buffer,0,$read);$received+=$read;if($received -gt [long]$m.bytes){throw 'Download exceeded expected size'};$pct=[int]($received*100/[long]$m.bytes);if($pct -ne $last){Write-Output ('STATUS:Downloading update... '+$pct+'%');$last=$pct}}
 }finally{$file.Dispose();$stream.Dispose();$response.Dispose()}
 Write-Output 'STATUS:Checking SHA-256 and unpacking...' 
 if((Get-Item $zip).Length -ne [long]$m.bytes){throw 'Download size mismatch'}
 if((Get-FileHash $zip -Algorithm SHA256).Hash -ne $m.sha256){throw 'Download hash mismatch'}
 Add-Type -AssemblyName System.IO.Compression.FileSystem
 $archive=[IO.Compression.ZipFile]::OpenRead($zip)
 try{
  $names=@('ZimClip.exe','capture-engine.exe','media-analysis.exe','media-export.exe','Source-and-licenses.zip','START-HERE.txt','SHA256SUMS.txt')
  foreach($name in $names){
   $matches=@($archive.Entries|Where-Object{[IO.Path]::GetFileName($_.FullName) -eq $name})
   if($matches.Count -gt 1){throw 'Duplicate update file'}
   if($matches.Count -eq 1){if($matches[0].Length -gt 100000000){throw 'Oversize file'};[IO.Compression.ZipFileExtensions]::ExtractToFile($matches[0],(Join-Path $d $name))}
  }
 }finally{$archive.Dispose()}
 foreach($required in $names){if(!(Test-Path (Join-Path $d $required))){throw 'Incomplete update'}}
 Write-Output $m.version;Write-Output $d
}catch{Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue;throw}
"#;
    let mut child = Command::new("powershell.exe")
        .creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("ZC_MANIFEST", MANIFEST)
        .env(
            "ZC_VERSION",
            env!("CARGO_PKG_VERSION").split('-').next().unwrap(),
        )
        .env("ZC_PRE", if env!("CARGO_PKG_VERSION").contains('-') { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(status) = line.strip_prefix("STATUS:") {
                progress.send(status.to_string()).ok();
            } else {
                text.push_str(&line);
                text.push('\n');
            }
        }
        text
    });
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        stderr.read_to_string(&mut text).ok();
        text
    });
    let began = std::time::Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.success(),
            Ok(None) => {}
            Err(e) => {
                child.kill().ok();
                child.wait().ok();
                return Err(e.to_string());
            }
        }
        if began.elapsed() > std::time::Duration::from_secs(240) {
            child.kill().ok();
            child.wait().ok();
            let _ = reader.join();
            let _ = errors.join();
            return Err("Update timed out after 4 minutes. Retry or use the ZIP. Current install unchanged.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let stdout = reader.join().unwrap_or_default();
    let stderr = errors.join().unwrap_or_default();
    if !success {
        return Err(stderr);
    }
    let output = stdout;
    let mut lines = output.lines().map(str::trim).filter(|s| !s.is_empty());
    let v = lines.next().ok_or("Missing update response")?;
    if v == "CURRENT" {
        return Ok(None);
    }
    let folder = PathBuf::from(lines.next().ok_or("Missing stage folder")?);
    Ok(Some(Staged {
        version: v.to_string(),
        folder,
    }))
}
pub fn apply(staged: &Staged, root: &Path) -> Result<(), String> {
    let script = r#"
$ErrorActionPreference='Stop'
$root=$env:ZC_ROOT;$stage=$env:ZC_STAGE
Wait-Process -Id ([int]$env:ZC_PID) -Timeout 120 -ErrorAction SilentlyContinue
if(Get-Process -Id ([int]$env:ZC_PID) -ErrorAction SilentlyContinue){exit 1}
$backup=Join-Path $root ('update-backup-'+(Get-Date -Format 'yyyyMMdd-HHmmss'))
$names=@('ZimClip.exe','capture-engine.exe','media-analysis.exe','media-export.exe','Source-and-licenses.zip','START-HERE.txt','SHA256SUMS.txt')
$changed=@()
try{
 New-Item -ItemType Directory $backup|Out-Null
 foreach($name in $names){$new=Join-Path $stage $name;$dest=Join-Path $root $name;if(Test-Path $new){if(Test-Path $dest){Copy-Item -LiteralPath $dest -Destination (Join-Path $backup $name)};$changed+=,$name;Copy-Item -LiteralPath $new -Destination $dest -Force}}
 Start-Process -FilePath (Join-Path $root 'ZimClip.exe') -WorkingDirectory $root
 Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
}catch{
 foreach($name in $changed){$old=Join-Path $backup $name;if(Test-Path $old){Copy-Item -LiteralPath $old -Destination (Join-Path $root $name) -Force}else{Remove-Item -LiteralPath (Join-Path $root $name) -Force -ErrorAction SilentlyContinue}}
 $_|Out-File (Join-Path $root 'update-error.txt');Add-Type -AssemblyName PresentationFramework;[System.Windows.MessageBox]::Show('Update failed. Old program files restored. See update-error.txt.','ZimClip update')|Out-Null
}
"#;
    Command::new("powershell.exe")
        .creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("ZC_ROOT", root)
        .env("ZC_STAGE", &staged.folder)
        .env("ZC_PID", std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Manifest-only check (a few hundred bytes, no download). Ok(Some(version)) when the
/// published version is newer than this build. The same host and version rules as `stage`.
pub fn peek() -> Result<Option<String>, String> {
    let script = r#"
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12
$m=Invoke-RestMethod -Uri $env:ZC_MANIFEST -MaximumRedirection 0 -TimeoutSec 20
if($m.version -notmatch '^\d+\.\d+(\.\d+)?$'){throw 'Invalid version'}
$versionText=[string]$m.version;if(($versionText.Split('.')).Count -eq 2){$versionText+='.0'}
if($env:ZC_PRE -eq '1'){$stale=([version]$versionText -lt [version]$env:ZC_VERSION)}else{$stale=([version]$versionText -le [version]$env:ZC_VERSION)}
if($stale){Write-Output 'CURRENT'}else{Write-Output $m.version}
"#;
    let mut child = Command::new("powershell.exe")
        .creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("ZC_MANIFEST", MANIFEST)
        .env("ZC_VERSION", env!("CARGO_PKG_VERSION").split('-').next().unwrap())
        .env("ZC_PRE", if env!("CARGO_PKG_VERSION").contains('-') { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let began = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
        if began.elapsed() > std::time::Duration::from_secs(40) {
            child.kill().ok();
            child.wait().ok();
            return Err("update check timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let mut text = String::new();
    if let Some(mut o) = child.stdout.take() {
        use std::io::Read;
        o.read_to_string(&mut text).ok();
    }
    let v = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if v == "CURRENT" || v.is_empty() {
        return Ok(None);
    }
    if !v.chars().all(|c| c.is_ascii_digit() || c == '.') || v.len() > 16 {
        return Err("unexpected version text".into());
    }
    Ok(Some(v.to_string()))
}

/// "Skip this version" / "Remind me later" memory: update-notice.txt in the app folder, two
/// lines (`skip=<version>`, `remind=<unix seconds>`), written atomically. Only the notice reads
/// it; the manual Settings button ignores it.
pub const REMIND_SECS: u64 = 24 * 3600;
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
fn read_state(root: &Path) -> (String, u64) {
    let text = std::fs::read_to_string(root.join("update-notice.txt")).unwrap_or_default();
    let mut skip = String::new();
    let mut remind = 0;
    for l in text.lines() {
        if let Some(v) = l.strip_prefix("skip=") {
            skip = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("remind=") {
            remind = v.trim().parse().unwrap_or(0);
        }
    }
    (skip, remind)
}
fn write_state(root: &Path, skip: &str, remind: u64) -> Result<(), String> {
    let tmp = root.join("update-notice.txt.tmp");
    std::fs::write(&tmp, format!("skip={skip}\nremind={remind}\n")).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, root.join("update-notice.txt")).map_err(|e| e.to_string())
}
/// True when the notice may be shown for `version`.
pub fn notice_due(root: &Path, version: &str) -> bool {
    let (skip, remind) = read_state(root);
    skip != version && now() >= remind
}
pub fn remind_later(root: &Path, version: &str) -> Result<(), String> {
    let (skip, _) = read_state(root);
    let keep = if skip == version { skip } else { String::new() };
    write_state(root, &keep, now() + REMIND_SECS)
}
pub fn skip_version(root: &Path, version: &str) -> Result<(), String> {
    write_state(root, version, 0)
}
