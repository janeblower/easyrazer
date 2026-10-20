use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static CHANGING: AtomicBool = AtomicBool::new(false);

#[derive(serde::Serialize)]
pub struct Status {
    pub installed: bool,
    pub ready: bool,
}

fn packages(directory: &Path) -> io::Result<(Vec<PathBuf>, bool)> {
    let (mut main, mut xusb) = (false, false);
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
        if !name.strip_prefix("oem").and_then(|n| n.strip_suffix(".inf"))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) { continue; }
        // Another vendor's locked or broken INF says nothing about these packages.
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let content = if bytes.starts_with(&[0xff, 0xfe]) {
            String::from_utf16_lossy(&bytes[2..].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect::<Vec<_>>())
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        let (mut version, mut component, mut pinned) = (false, None, false);
        for line in content.lines() {
            let line = line.split(';').next().unwrap_or_default().trim();
            if line.starts_with('[') {
                version = line.eq_ignore_ascii_case("[Version]");
                continue;
            }
            if !version { continue; }
            let Some((key, value)) = line.split_once('=') else { continue; };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().trim_matches('"').to_ascii_lowercase();
            if key == "catalogfile" || key.starts_with("catalogfile.") {
                component = match value.as_str() {
                    "hidmaestro.cat" => Some(false),
                    "hidmaestro_xusb.cat" => Some(true),
                    _ => component,
                };
            }
            if key == "driverver" {
                pinned = value.replace([' ', '\t'], "") == "10/13/2026,1.12.0.2302";
            }
        }
        if let Some(companion) = component {
            paths.push(path);
            if companion { xusb |= pinned; } else { main |= pinned; }
        }
    }
    Ok((paths, main && xusb))
}

fn inf_directory() -> Result<PathBuf, String> {
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing")?;
    Ok(PathBuf::from(root).join("INF"))
}

pub fn status() -> Result<Status, String> {
    let (paths, ready) = packages(&inf_directory()?).map_err(|e| e.to_string())?;
    Ok(Status { installed: !paths.is_empty(), ready })
}

pub fn installed() -> Result<bool, String> {
    Ok(status()?.ready)
}

struct Change;
impl Change {
    fn start() -> Result<Self, String> {
        if CHANGING.swap(true, Ordering::SeqCst) {
            return Err("HIDMaestro installation or removal is already running".into());
        }
        Ok(Self)
    }
}
impl Drop for Change {
    fn drop(&mut self) { CHANGING.store(false, Ordering::SeqCst); }
}

pub fn install() -> Result<(), String> {
    let _change = Change::start()?;
    super::process::install()?;
    if !installed()? { return Err("HIDMaestro driver packages were not installed".into()); }
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    let _change = Change::start()?;
    super::process::uninstall()?;
    if status()?.installed { return Err("Some HIDMaestro driver packages are still installed".into()); }
    Ok(())
}

pub(super) fn remove() -> Result<(), String> {
    use crate::autostart::wide;
    use windows_sys::Win32::{
        Devices::DeviceAndDriverInstallation::DiUninstallDriverW,
        Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
        System::Threading::CreateMutexW,
    };
    let owner = super::ipc::owned(unsafe {
        CreateMutexW(std::ptr::null(), 0, wide("Global\\HIDMaestroRustController0").as_ptr())
    }).map_err(|e| e.to_string())?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return Err("Disable the gamepad before uninstalling HIDMaestro".into());
    }
    let mut reboot = false;
    for path in packages(&inf_directory()?).map_err(|e| e.to_string())?.0 {
        let mut needed = 0;
        if unsafe { DiUninstallDriverW(std::ptr::null_mut(), wide(&path.to_string_lossy()).as_ptr(), 0, &mut needed) } == 0 {
            return Err(format!("HIDMaestro removal: {}", io::Error::last_os_error()));
        }
        reboot |= needed != 0;
    }
    drop(owner);
    if reboot { return Err("Restart Windows to finish uninstalling HIDMaestro".into()); }
    Ok(())
}

pub(super) fn deploy() -> Result<(), String> {
    run(include_str!("install.ps1"))
}

fn run(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing")?;
    let output = Command::new(std::path::PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x08000000)
        .output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("HIDMaestro installation failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_and_removal_cannot_overlap() {
        let change = Change::start().unwrap();
        assert!(install().is_err());
        assert!(uninstall().is_err());
        drop(change);
        assert!(Change::start().is_ok());
    }

    #[test]
    #[ignore = "explicit elevated test reserves the controller lease without removing drivers"]
    fn removal_refuses_an_active_controller() {
        use windows_sys::Win32::System::Threading::CreateMutexW;
        let before = packages(&inf_directory().unwrap()).unwrap().0;
        let owner = super::super::ipc::owned(unsafe {
            CreateMutexW(std::ptr::null(), 0, crate::autostart::wide("Global\\HIDMaestroRustController0").as_ptr())
        }).unwrap();
        let error = remove().unwrap_err();
        assert!(error.contains("Disable the gamepad"), "{error}");
        assert_eq!(packages(&inf_directory().unwrap()).unwrap().0, before);
        drop(owner);
    }

    #[test]
    fn references_in_comments_do_not_identify_driver_packages() {
        let dir = std::env::temp_dir().join(format!("easyrazer-comment-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, dll) in [("oem1.inf", "hidmaestro.dll"), ("oem2.inf", "hmxinput.dll")] {
            std::fs::write(dir.join(file), format!("DriverVer=10/13/2026,1.12.0.2302\n; hidmaestro {dll}\nCatalogFile=another.cat")).unwrap();
        }
        let detected = packages(&dir).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(detected.0.is_empty());
    }

    #[test]
    fn an_unreadable_entry_does_not_hide_the_packages() {
        let dir = std::env::temp_dir().join(format!("easyrazer-unreadable-check-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("oem0.inf")).unwrap();
        std::fs::write(dir.join("oem1.inf"), "[Version]\nDriverVer=10/13/2026,1.12.0.2302\nCatalogFile=hidmaestro.cat").unwrap();
        std::fs::write(dir.join("oem2.inf"), "[Version]\nDriverVer=10/13/2026,1.12.0.2302\nCatalogFile=hidmaestro_xusb.cat").unwrap();
        let detected = packages(&dir);
        std::fs::remove_dir_all(dir).unwrap();
        assert!(detected.unwrap().1);
    }

    #[test]
    fn detection_requires_both_pinned_driver_packages() {
        let dir = std::env::temp_dir().join(format!("easyrazer-driver-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!packages(&dir).unwrap().1);
        std::fs::write(dir.join("oem1.inf"), "[Version]\nDriverVer = 10/13/2026,1.12.0.2302\nCatalogFile=hidmaestro.cat").unwrap();
        assert!(!packages(&dir).unwrap().1);
        std::fs::write(dir.join("oem2.inf"), "[Version]\nDriverVer=10/13/2026,1.11.0.0\nCatalogFile=hidmaestro_xusb.cat").unwrap();
        let (paths, ready) = packages(&dir).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(!ready);
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend("[Version]\nDriverVer = 10/13/2026,1.12.0.2302\r\nCatalogFile = hidmaestro_xusb.cat".encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(dir.join("oem2.inf"), bytes).unwrap();
        assert!(packages(&dir).unwrap().1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn installer_rejects_an_unverified_download_before_deployment() {
        let dir = std::env::temp_dir().join(format!("easyrazer-install-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = format!(
            "function Invoke-WebRequest {{ param([switch]$UseBasicParsing, [int]$TimeoutSec, [string]$Uri, [string]$OutFile) [IO.File]::WriteAllText($OutFile, 'bad download') }}\n{}",
            include_str!("install.ps1").replace("$env:ProgramFiles", "$env:EASYRAZER_INSTALL_TEST"),
        );
        let script = format!("$env:EASYRAZER_INSTALL_TEST = '{}'\n{script}", dir.to_string_lossy().replace('\'', "''"));
        let error = run(&script).unwrap_err();
        assert!(error.contains("checksum mismatch"), "{error}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir(dir).unwrap();
    }
}
