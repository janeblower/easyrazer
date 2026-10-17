use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static INSTALLING: AtomicBool = AtomicBool::new(false);

fn packages(directory: &Path) -> io::Result<bool> {
    let (mut main, mut xusb) = (false, false);
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
        if !name.starts_with("oem") || !name.ends_with(".inf") { continue; }
        let bytes = std::fs::read(path)?;
        let content = if bytes.starts_with(&[0xff, 0xfe]) {
            String::from_utf16_lossy(&bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        let compact = content.to_ascii_lowercase().replace([' ', '\t', '\r'], "");
        if compact.contains("driverver=10/13/2026,1.12.0.2302") && compact.contains("hidmaestro") {
            main |= compact.contains("hidmaestro.dll");
            xusb |= compact.contains("hmxinput.dll");
        }
    }
    Ok(main && xusb)
}

pub fn installed() -> Result<bool, String> {
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing")?;
    packages(&std::path::PathBuf::from(root).join("INF")).map_err(|e| e.to_string())
}

pub fn install() -> Result<(), String> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("HIDMaestro installation is already running".into());
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) { INSTALLING.store(false, Ordering::SeqCst); }
    }
    let _guard = Guard;
    super::process::install()?;
    if !installed()? { return Err("HIDMaestro driver packages were not installed".into()); }
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
    fn detection_requires_both_pinned_driver_packages() {
        let dir = std::env::temp_dir().join(format!("easyrazer-driver-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!packages(&dir).unwrap());
        std::fs::write(dir.join("oem1.inf"), "DriverVer = 10/13/2026,1.12.0.2302\nHIDMaestro.dll").unwrap();
        assert!(!packages(&dir).unwrap());
        std::fs::write(dir.join("oem2.inf"), "DriverVer=10/13/2026,1.11.0.0\nHMXInput.dll\nHIDMaestro").unwrap();
        assert!(!packages(&dir).unwrap());
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend("DriverVer = 10/13/2026,1.12.0.2302\r\nHMXInput.dll\nHIDMaestro".encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(dir.join("oem2.inf"), bytes).unwrap();
        assert!(packages(&dir).unwrap());
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
