use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static CHANGING: AtomicBool = AtomicBool::new(false);

type Version = [u32; 4];

/// The driver the bundled client is built for; later drivers serve its protocol as well.
const REQUIRED: Version = [2, 0, 0, 2201];

#[derive(serde::Serialize)]
pub struct Status {
    pub installed: bool,
    pub ready: bool,
    /// The older of the two packages' versions, once both are installed.
    pub version: Option<String>,
}

/// The version half of a `DriverVer = date,version` value.
fn driver_version(value: &str) -> Option<Version> {
    let (_, version) = value.split_once(',')?;
    let mut parts = [0; 4];
    for (part, text) in parts.iter_mut().zip(version.split('.')) {
        *part = text.trim().parse().ok()?;
    }
    Some(parts)
}

/// The driver store keeps older packages next to newer ones, and Windows binds the newest,
/// so each component counts at its highest version.
fn packages(directory: &Path) -> io::Result<(Vec<PathBuf>, Option<Version>)> {
    let (mut main, mut xusb) = (None, None);
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
        let (mut section, mut component, mut found) = (false, None, None);
        for line in content.lines() {
            let line = line.split(';').next().unwrap_or_default().trim();
            if line.starts_with('[') {
                section = line.eq_ignore_ascii_case("[Version]");
                continue;
            }
            if !section { continue; }
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
                found = driver_version(&value);
            }
        }
        if let Some(companion) = component {
            paths.push(path);
            let best = if companion { &mut xusb } else { &mut main };
            *best = (*best).max(found);
        }
    }
    Ok((paths, main.zip(xusb).map(|(main, xusb)| main.min(xusb))))
}

fn inf_directory() -> Result<PathBuf, String> {
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing")?;
    Ok(PathBuf::from(root).join("INF"))
}

pub fn status() -> Result<Status, String> {
    let (paths, version) = packages(&inf_directory()?).map_err(|e| e.to_string())?;
    Ok(Status {
        installed: !paths.is_empty(),
        ready: version.is_some_and(|v| v >= REQUIRED),
        version: version.map(|v| v.map(|part| part.to_string()).join(".")),
    })
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
    let (owner, existed) = crate::sys::create_mutex("Global\\HIDMaestroRustController0").map_err(|e| e.to_string())?;
    if existed {
        return Err("Disable the gamepad before uninstalling HIDMaestro".into());
    }
    let mut reboot = false;
    for path in packages(&inf_directory()?).map_err(|e| e.to_string())?.0 {
        reboot |= crate::sys::uninstall_driver(&path).map_err(|e| format!("HIDMaestro removal: {e}"))?;
    }
    run(UNTRUST).map_err(|e| format!("HIDMaestro certificate removal: {e}"))?;
    drop(owner);
    if reboot { return Err("Restart Windows to finish uninstalling HIDMaestro".into()); }
    Ok(())
}

/// The installer's signing certificate is trusted only while its drivers are installed.
const UNTRUST: &str = r#"$ErrorActionPreference = 'Stop'
foreach ($name in @('My', 'Root', 'TrustedPublisher')) {
    Get-ChildItem "Cert:\LocalMachine\$name" | Where-Object Subject -eq 'CN=HIDMaestroTestCert' | ForEach-Object {
        # Root and TrustedPublisher copies may still point at the key deleted with the My one.
        if ($name -eq 'My') { Remove-Item -LiteralPath $_.PSPath -DeleteKey } else { Remove-Item -LiteralPath $_.PSPath }
    }
}"#;

pub(super) fn deploy() -> Result<(), String> {
    run(include_str!("install.ps1")).map_err(|e| format!("HIDMaestro installation failed: {e}"))
}

fn run(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing")?;
    let output = Command::new(std::path::PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        // PowerShell 7's module path hides Windows PowerShell's own modules, Get-FileHash among them.
        .env_remove("PSModulePath")
        .creation_flags(0x08000000)
        .output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
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
        let before = packages(&inf_directory().unwrap()).unwrap().0;
        let (owner, _) = crate::sys::create_mutex("Global\\HIDMaestroRustController0").unwrap();
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
            std::fs::write(dir.join(file), format!("DriverVer=10/19/2026,2.0.0.2201\n; hidmaestro {dll}\nCatalogFile=another.cat")).unwrap();
        }
        let detected = packages(&dir).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(detected.0.is_empty());
    }

    #[test]
    fn an_unreadable_entry_does_not_hide_the_packages() {
        let dir = std::env::temp_dir().join(format!("easyrazer-unreadable-check-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("oem0.inf")).unwrap();
        std::fs::write(dir.join("oem1.inf"), "[Version]\nDriverVer=10/19/2026,2.0.0.2201\nCatalogFile=hidmaestro.cat").unwrap();
        std::fs::write(dir.join("oem2.inf"), "[Version]\nDriverVer=10/19/2026,2.0.0.2201\nCatalogFile=hidmaestro_xusb.cat").unwrap();
        let detected = packages(&dir);
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(detected.unwrap().1, Some(REQUIRED));
    }

    #[test]
    fn detection_reports_the_older_of_both_driver_packages() {
        let dir = std::env::temp_dir().join(format!("easyrazer-driver-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(packages(&dir).unwrap().1, None);
        std::fs::write(dir.join("oem1.inf"), "[Version]\nDriverVer = 10/19/2026,2.0.0.2201\nCatalogFile=hidmaestro.cat").unwrap();
        assert_eq!(packages(&dir).unwrap().1, None);
        std::fs::write(dir.join("oem2.inf"), "[Version]\nDriverVer=10/13/2026,1.11.0.0\nCatalogFile=hidmaestro_xusb.cat").unwrap();
        let (paths, version) = packages(&dir).unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(version, Some([1, 11, 0, 0]));
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend("[Version]\nDriverVer = 10/19/2026,2.0.0.2201\r\nCatalogFile = hidmaestro_xusb.cat".encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(dir.join("oem2.inf"), bytes).unwrap();
        assert_eq!(packages(&dir).unwrap().1, Some(REQUIRED));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_newer_package_outranks_an_older_one_left_in_the_store() {
        let dir = std::env::temp_dir().join(format!("easyrazer-driver-newer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, version, catalog) in [
            ("oem1.inf", "1.12.0.2302", "hidmaestro.cat"),
            ("oem2.inf", "1.12.0.2302", "hidmaestro_xusb.cat"),
            ("oem3.inf", "2.1.0.0", "hidmaestro.cat"),
            ("oem4.inf", "2.0.1.5", "hidmaestro_xusb.cat"),
        ] {
            std::fs::write(dir.join(file), format!("[Version]\nDriverVer=10/30/2026,{version}\nCatalogFile={catalog}")).unwrap();
        }
        let detected = packages(&dir).unwrap().1;
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(detected, Some([2, 0, 1, 5]));
        assert!(detected.unwrap() > REQUIRED);
    }

    #[test]
    fn driver_versions_compare_by_number() {
        assert_eq!(driver_version("10/19/2026, 2.0.0.2201"), Some(REQUIRED));
        assert_eq!(driver_version("10/13/2026,1.12.0.2302"), Some([1, 12, 0, 2302]));
        assert!(driver_version("10/13/2026,1.12.0.2302").unwrap() < REQUIRED);
        assert!(driver_version("01/01/2027,2.0.0.10000").unwrap() > REQUIRED);
        assert_eq!(driver_version("10/19/2026,2.0"), Some([2, 0, 0, 0]));
        assert_eq!(driver_version("10/19/2026"), None);
        assert_eq!(driver_version("10/19/2026,two"), None);
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
