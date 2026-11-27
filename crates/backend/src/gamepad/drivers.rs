//! hidshim, the driver behind the gamepad, and HIDMaestro, the one earlier versions used.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;

static CHANGING: AtomicBool = AtomicBool::new(false);

type Version = [u32; 3];

/// The oldest hidshim the app supports: the first that takes reports through shared memory.
const REQUIRED: Version = [0, 3, 0];
const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\hidshim";
const CREATE_NO_WINDOW: u32 = 0x08000000;
const RELEASES: &str = "/janeblower/hidshim/releases/latest/download/";
const DOWNLOAD_LIMIT: usize = 16 << 20;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Status {
    pub installed: bool,
    pub ready: bool,
    /// The installed hidshim's version, when it was installed as a program.
    pub version: Option<String>,
    /// HIDMaestro driver packages are still installed.
    pub legacy: bool,
    /// Whether hidshim brings its pads back on boot; none when the installed one cannot.
    pub boot: Option<bool>,
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

/// The boot task as the installed program reports it, which needs no elevation.
fn boot_task(exe: &Path, version: Option<Version>) -> Option<bool> {
    if version.is_none_or(|v| v < REQUIRED) { return None; }
    let output = Command::new(exe).arg("autostart").creation_flags(CREATE_NO_WINDOW).output().ok()?;
    if !output.status.success() { return None; }
    parse_boot(&String::from_utf8_lossy(&output.stdout))
}

fn parse_boot(text: &str) -> Option<bool> {
    match text.trim() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

pub fn status() -> Result<Status, String> {
    let installed = hidshim::status().map_err(|e| e.to_string())? != hidshim::Status::NotInstalled;
    let program = program();
    let version = program.as_ref().and_then(|(_, v)| *v);
    Ok(Status {
        installed,
        // A hand-installed driver has no program to compare; the pad checks its protocol on open.
        ready: installed && version.is_none_or(|v| v >= REQUIRED),
        version: version.map(|v| v.map(|part| part.to_string()).join(".")),
        legacy: !legacy_packages(&inf_directory()?).map_err(|e| e.to_string())?.is_empty(),
        boot: program.filter(|_| installed).and_then(|(exe, v)| boot_task(&exe, v)),
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

/// Turns hidshim's boot task on or off.
pub fn set_boot(on: bool) -> Result<(), String> {
    let _change = Change::start()?;
    super::process::driver_action(if on { "--gamepad-boot-on" } else { "--gamepad-boot-off" })
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
/// otherwise the latest release.
pub(super) fn deploy(replace: bool) -> Result<(), String> {
    if replace {
        remove_legacy()?;
    }
    match program() {
        Some((exe, Some(version))) if version >= REQUIRED => run_program(&exe, install_args(boot_task(&exe, Some(version)) == Some(true))),
        _ => install_latest(),
    }
}

/// Installs without the boot task; the user turns it on in the settings.
fn install_latest() -> Result<(), String> {
    let exe = download_latest()?;
    let stage = Stage::create()?;
    let path = stage.0.join("hidshim.exe");
    std::fs::write(&path, exe).map_err(|e| format!("hidshim download: {e}"))?;
    // hidshim copies itself to Program Files and lists itself among installed apps.
    run_program(&path, &["install", "--no-autostart"])
}

/// The latest hidshim.exe, checked against its release's checksums.
fn download_latest() -> Result<Vec<u8>, String> {
    let download = |name: &str| {
        crate::sys::download("github.com", &format!("{RELEASES}{name}"), DOWNLOAD_LIMIT)
            .map_err(|e| format!("hidshim download: {e}"))
    };
    let exe = download("hidshim.exe")?;
    let sums = String::from_utf8_lossy(&download("SHA256SUMS")?).into_owned();
    let expected = checksum(&sums, "hidshim.exe").ok_or("hidshim release lists no checksum")?;
    let actual: String = crate::sys::sha256(&exe).map_err(|e| e.to_string())?.iter().map(|b| format!("{b:02x}")).collect();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err("hidshim release checksum mismatch".into());
    }
    Ok(exe)
}

/// The checksum a `sha256sum` listing gives the file.
fn checksum<'a>(sums: &'a str, file: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        (name.trim_start().trim_start_matches('*') == file && hash.len() == 64).then_some(hash)
    })
}

/// A fresh directory only administrators can write to, so nobody swaps the checked program
/// before it runs elevated.
struct Stage(PathBuf);

impl Stage {
    fn create() -> Result<Self, String> {
        let mut token = [0u8; 16];
        getrandom::fill(&mut token).map_err(|e| e.to_string())?;
        let name: String = token.iter().map(|b| format!("{b:02x}")).collect();
        let path = program_files()?.join(format!("EasyRazer.DriverInstall.{name}"));
        std::fs::create_dir(&path).map_err(|e| format!("hidshim download: {e}"))?;
        Ok(Self(path))
    }
}

impl Drop for Stage {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

/// From the registry, not %ProgramFiles%: the elevated helper inherits the user's environment.
fn program_files() -> Result<PathBuf, String> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    let key = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion", KEY_READ | KEY_WOW64_64KEY)
        .map_err(|e| e.to_string())?;
    key.get_value::<String, _>("ProgramFilesDir").map(PathBuf::from).map_err(|e| format!("Program Files: {e}"))
}

/// An update keeps the boot task as the user set it; a fresh install comes without one.
fn install_args(boot: bool) -> &'static [&'static str] {
    if boot { &["install"] } else { &["install", "--no-autostart"] }
}

/// Runs elevated.
pub(super) fn boot(on: bool) -> Result<(), String> {
    match program() {
        Some((exe, Some(version))) if version >= REQUIRED => run_program(&exe, &["autostart", if on { "on" } else { "off" }]),
        _ => Err("Update hidshim to start its driver with Windows".into()),
    }
}

/// Runs elevated.
pub(super) fn remove() -> Result<(), String> {
    match program() {
        Some((exe, _)) => run_program(&exe, &["uninstall"]),
        None => Err("hidshim was installed by hand; remove it the way it was installed".into()),
    }
}

fn run_program(exe: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new(exe).args(args).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("hidshim {}: {}", args[0], error.trim().trim_start_matches("error: ")));
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
    // HIDMaestro's installer trusted its signing certificate only while its drivers were installed.
    for store in ["My", "Root", "TrustedPublisher"] {
        // Root and TrustedPublisher copies may still point at the key deleted with the My one.
        crate::sys::remove_machine_certificates(store, "HIDMaestroTestCert", store == "My")
            .map_err(|e| format!("HIDMaestro certificate removal: {e}"))?;
    }
    drop(owner);
    if reboot { return Err("Restart Windows to finish removing HIDMaestro, then install hidshim".into()); }
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
        assert_eq!(parse_version("0.3.0"), Some(REQUIRED));
        assert!(parse_version("0.10.0").unwrap() > REQUIRED);
        assert!(parse_version("0.2.9").unwrap() < REQUIRED);
        for bad in ["", "0.1", "0.1.0.0", "0.x.0"] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
    }

    #[test]
    fn boot_task_state_is_read_from_the_program_output() {
        assert_eq!(parse_boot("on
"), Some(true));
        assert_eq!(parse_boot("off
"), Some(false));
        assert_eq!(parse_boot("usage: hidshim <command>"), None);
        assert_eq!(boot_task(Path::new("hidshim.exe"), Some([0, 1, 0])), None);
        assert_eq!(boot_task(Path::new("hidshim.exe"), None), None);
    }

    #[test]
    fn updates_keep_the_boot_task_and_fresh_installs_leave_it_out() {
        assert_eq!(install_args(true), ["install"]);
        assert_eq!(install_args(false), ["install", "--no-autostart"]);
    }

    #[test]
    #[ignore = "downloads the latest hidshim release"]
    fn latest_release_matches_its_checksum() {
        let exe = download_latest().unwrap();
        assert!(exe.starts_with(b"MZ"));
    }

    #[test]
    #[ignore = "elevated: creates a directory in Program Files"]
    fn stage_is_removed_after_use() {
        let stage = Stage::create().unwrap();
        let path = stage.0.clone();
        std::fs::write(path.join("hidshim.exe"), b"MZ").unwrap();
        assert!(path.starts_with(program_files().unwrap()));
        drop(stage);
        assert!(!path.exists());
    }

    #[test]
    fn checksums_are_found_by_file_name() {
        let hash = "4feaab47a7f75c7fb50c66556dc62ebefdcca8cd4d7221cc81c93293b2d45137";
        let sums = format!("{}  hidshim-drivers.zip
{hash}  hidshim.exe
", "5".repeat(64));
        assert_eq!(checksum(&sums, "hidshim.exe"), Some(hash));
        assert_eq!(checksum(&format!("{hash} *hidshim.exe"), "hidshim.exe"), Some(hash));
        assert_eq!(checksum(&sums, "hidshim"), None);
        assert_eq!(checksum("abc  hidshim.exe", "hidshim.exe"), None);
    }
}
