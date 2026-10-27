//! hidshim, the driver behind the gamepad, and HIDMaestro, the one earlier versions used.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;

static CHANGING: AtomicBool = AtomicBool::new(false);

type Version = [u32; 3];

/// The hidshim release install.ps1 downloads; later releases serve its protocol as well.
const REQUIRED: Version = [0, 1, 0];
const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\hidshim";
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(serde::Serialize)]
pub struct Status {
    pub installed: bool,
    pub ready: bool,
    /// The installed hidshim's version, when it was installed as a program.
    pub version: Option<String>,
    /// HIDMaestro driver packages are still installed.
    pub legacy: bool,
}

fn parse_version(text: &str) -> Option<Version> {
    let mut parts = [0; 3];
    let mut split = text.trim().split('.');
    for part in &mut parts {
        *part = split.next()?.parse().ok()?;
    }
    split.next().is_none().then_some(parts)
}

/// The installed program, from a key only administrators can write: the elevated helper runs it.
fn program() -> Option<(PathBuf, Option<Version>)> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    let key = winreg::RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(UNINSTALL_KEY, KEY_READ | KEY_WOW64_64KEY).ok()?;
    let location: String = key.get_value("InstallLocation").ok()?;
    let exe = Path::new(&location).join("hidshim.exe");
    let version = key.get_value::<String, _>("DisplayVersion").ok().and_then(|v| parse_version(&v));
    exe.is_file().then_some((exe, version))
}

/// HIDMaestro's driver packages, by the catalogs its installer names.
fn legacy_packages(directory: &Path) -> io::Result<Vec<PathBuf>> {
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
        let mut section = false;
        let ours = content.lines().any(|line| {
            let line = line.split(';').next().unwrap_or_default().trim();
            if line.starts_with('[') {
                section = line.eq_ignore_ascii_case("[Version]");
                return false;
            }
            let Some((key, value)) = line.split_once('=').filter(|_| section) else { return false };
            let key = key.trim().to_ascii_lowercase();
            (key == "catalogfile" || key.starts_with("catalogfile."))
                && matches!(value.trim().trim_matches('"').to_ascii_lowercase().as_str(), "hidmaestro.cat" | "hidmaestro_xusb.cat")
        });
        if ours { paths.push(path); }
    }
    Ok(paths)
}

fn inf_directory() -> Result<PathBuf, String> {
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing")?;
    Ok(PathBuf::from(root).join("INF"))
}

pub fn status() -> Result<Status, String> {
    let installed = hidshim::status().map_err(|e| e.to_string())? != hidshim::Status::NotInstalled;
    let version = program().and_then(|(_, v)| v);
    Ok(Status {
        installed,
        // A hand-installed driver has no program to compare; the pad checks its protocol on open.
        ready: installed && version.is_none_or(|v| v >= REQUIRED),
        version: version.map(|v| v.map(|part| part.to_string()).join(".")),
        legacy: !legacy_packages(&inf_directory()?).map_err(|e| e.to_string())?.is_empty(),
    })
}

struct Change;
impl Change {
    fn start() -> Result<Self, String> {
        if CHANGING.swap(true, Ordering::SeqCst) {
            return Err("hidshim installation or removal is already running".into());
        }
        Ok(Self)
    }
}
impl Drop for Change {
    fn drop(&mut self) { CHANGING.store(false, Ordering::SeqCst); }
}

/// Installs or updates hidshim, or brings its pad back after a reboot. `replace` removes
/// HIDMaestro first.
pub fn install(replace: bool) -> Result<(), String> {
    let _change = Change::start()?;
    super::process::driver_action(if replace { "--gamepad-replace" } else { "--gamepad-install" })?;
    if hidshim::status().map_err(|e| e.to_string())? != hidshim::Status::Ready {
        return Err("hidshim was installed, but its pad is not running".into());
    }
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    let _change = Change::start()?;
    super::process::driver_action("--gamepad-uninstall")?;
    if status()?.installed { return Err("hidshim is still installed".into()); }
    Ok(())
}

/// Brings the pad back with one elevation prompt and opens it.
pub(super) fn restore() -> Receiver<Result<hidshim::Gamepad, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = install(false).and_then(|()| hidshim::Gamepad::open().map_err(|e| e.to_string()));
        let _ = tx.send(result);
        crate::wake::wake();
    });
    rx
}

/// Runs elevated: the installed program when it is new enough, which needs no network,
/// otherwise the pinned release.
pub(super) fn deploy(replace: bool) -> Result<(), String> {
    if replace {
        remove_legacy()?;
    }
    match program() {
        Some((exe, Some(version))) if version >= REQUIRED => run_program(&exe, "install"),
        _ => run(include_str!("install.ps1")).map_err(|e| format!("hidshim installation failed: {e}")),
    }
}

/// Runs elevated.
pub(super) fn remove() -> Result<(), String> {
    match program() {
        Some((exe, _)) => run_program(&exe, "uninstall"),
        None => Err("hidshim was installed by hand; remove it the way it was installed".into()),
    }
}

fn run_program(exe: &Path, command: &str) -> Result<(), String> {
    let output = Command::new(exe).arg(command).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("hidshim {command}: {}", error.trim().trim_start_matches("error: ")));
    }
    Ok(())
}

fn remove_legacy() -> Result<(), String> {
    let (owner, existed) = crate::sys::create_mutex("Global\\HIDMaestroRustController0").map_err(|e| e.to_string())?;
    if existed {
        return Err("Close programs that use HIDMaestro before removing it".into());
    }
    let mut reboot = false;
    for path in legacy_packages(&inf_directory()?).map_err(|e| e.to_string())? {
        reboot |= crate::sys::uninstall_driver(&path).map_err(|e| format!("HIDMaestro removal: {e}"))?;
    }
    run(UNTRUST).map_err(|e| format!("HIDMaestro certificate removal: {e}"))?;
    drop(owner);
    if reboot { return Err("Restart Windows to finish removing HIDMaestro, then install hidshim".into()); }
    Ok(())
}

/// HIDMaestro's installer trusted its signing certificate only while its drivers were installed.
const UNTRUST: &str = r#"$ErrorActionPreference = 'Stop'
foreach ($name in @('My', 'Root', 'TrustedPublisher')) {
    Get-ChildItem "Cert:\LocalMachine\$name" | Where-Object Subject -eq 'CN=HIDMaestroTestCert' | ForEach-Object {
        # Root and TrustedPublisher copies may still point at the key deleted with the My one.
        if ($name -eq 'My') { Remove-Item -LiteralPath $_.PSPath -DeleteKey } else { Remove-Item -LiteralPath $_.PSPath }
    }
}"#;

fn run(script: &str) -> Result<(), String> {
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing")?;
    let output = Command::new(PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        // PowerShell 7's module path hides Windows PowerShell's own modules, Get-FileHash among them.
        .env_remove("PSModulePath")
        .creation_flags(CREATE_NO_WINDOW)
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
        assert!(install(false).is_err());
        assert!(uninstall().is_err());
        drop(change);
        assert!(Change::start().is_ok());
    }

    #[test]
    #[ignore = "explicit elevated test reserves the controller lease without removing drivers"]
    fn legacy_removal_refuses_an_active_controller() {
        let before = legacy_packages(&inf_directory().unwrap()).unwrap();
        let (owner, _) = crate::sys::create_mutex("Global\\HIDMaestroRustController0").unwrap();
        let error = remove_legacy().unwrap_err();
        assert!(error.contains("Close programs"), "{error}");
        assert_eq!(legacy_packages(&inf_directory().unwrap()).unwrap(), before);
        drop(owner);
    }

    #[test]
    fn references_in_comments_do_not_identify_legacy_packages() {
        let dir = std::env::temp_dir().join(format!("easyrazer-comment-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, dll) in [("oem1.inf", "hidmaestro.dll"), ("oem2.inf", "hmxinput.dll")] {
            std::fs::write(dir.join(file), format!("[Version]\n; hidmaestro {dll} hidmaestro.cat\nCatalogFile=another.cat")).unwrap();
        }
        let detected = legacy_packages(&dir).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(detected.is_empty());
    }

    #[test]
    fn legacy_packages_are_found_by_catalog_in_any_encoding() {
        let dir = std::env::temp_dir().join(format!("easyrazer-legacy-check-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("oem0.inf")).unwrap();
        std::fs::write(dir.join("oem1.inf"), "[Version]\nDriverVer=10/19/2026,2.0.0.2201\nCatalogFile=hidmaestro.cat").unwrap();
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend("[Version]\r\nCatalogFile = \"HIDMaestro_xusb.cat\"".encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(dir.join("oem2.inf"), bytes).unwrap();
        std::fs::write(dir.join("oem3.inf"), "[Strings]\nCatalogFile=hidmaestro.cat").unwrap();
        std::fs::write(dir.join("hidmaestro.inf"), "[Version]\nCatalogFile=hidmaestro.cat").unwrap();
        let mut detected = legacy_packages(&dir).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        detected.sort();
        let names: Vec<_> = detected.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["oem1.inf", "oem2.inf"]);
    }

    #[test]
    fn program_versions_compare_by_number() {
        assert_eq!(parse_version("0.1.0"), Some(REQUIRED));
        assert!(parse_version("0.10.0").unwrap() > REQUIRED);
        assert!(parse_version("0.0.9").unwrap() < REQUIRED);
        for bad in ["", "0.1", "0.1.0.0", "0.x.0"] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
    }

    #[test]
    fn installer_downloads_the_required_release() {
        let [major, minor, patch] = REQUIRED;
        assert!(include_str!("install.ps1").contains(&format!("/releases/download/{major}.{minor}.{patch}/hidshim.exe")));
    }

    #[test]
    fn installer_rejects_an_unverified_download_before_running_it() {
        let dir = std::env::temp_dir().join(format!("easyrazer-install-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = "$base = [Environment]::GetFolderPath('ProgramFiles')";
        assert!(include_str!("install.ps1").contains(base));
        let script = format!(
            "function Invoke-WebRequest {{ param([switch]$UseBasicParsing, [int]$TimeoutSec, [string]$Uri, [string]$OutFile) [IO.File]::WriteAllText($OutFile, 'bad download') }}\n{}",
            include_str!("install.ps1").replace(base, "$base = $env:EASYRAZER_INSTALL_TEST"),
        );
        let script = format!("$env:EASYRAZER_INSTALL_TEST = '{}'\n{script}", dir.to_string_lossy().replace('\'', "''"));
        let error = run(&script).unwrap_err();
        assert!(error.contains("checksum mismatch"), "{error}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir(dir).unwrap();
    }
}
