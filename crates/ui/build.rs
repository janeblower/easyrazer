use std::path::PathBuf;
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-env-changed=EASYRAZER_VERSION");
    println!("cargo:rerun-if-env-changed=EASYRAZER_LICENSES");
    licenses();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        resources();
    }
}

/// The third-party notices a release is built with, for the About window; empty otherwise.
fn licenses() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("LICENSES.txt");
    let text = match env::var("EASYRAZER_LICENSES") {
        Ok(path) => {
            println!("cargo:rerun-if-changed={path}");
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("EASYRAZER_LICENSES {path}: {e}"))
        }
        Err(_) => String::new(),
    };
    fs::write(out, text).unwrap();
}

/// `1.2.3` as the four 16-bit fields of a Windows file version.
fn packed(version: &str) -> u64 {
    version.split('.').take(4).enumerate().fold(0, |v, (i, part)| v | (part.parse::<u64>().unwrap_or(0) & 0xFFFF) << (48 - 16 * i))
}

fn resources() {
    let version = env::var("EASYRAZER_VERSION").unwrap_or_else(|_| env::var("CARGO_PKG_VERSION").unwrap());
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico")
        .set_manifest_file("assets/easyrazer.manifest")
        .set("ProductName", "EasyRazer")
        .set("FileDescription", "EasyRazer")
        .set("OriginalFilename", "EasyRazer.exe")
        .set("ProductVersion", &version)
        .set("FileVersion", &version)
        .set_version_info(winresource::VersionInfo::FILEVERSION, packed(&version))
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed(&version));
    res.compile().expect("Windows resources");
}
