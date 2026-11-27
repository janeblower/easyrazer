//! Writes LICENSES.txt for a release: EasyRazer's license, the third-party crates by license
//! (from `cargo about`), their NOTICE files and the licenses of the bundled fonts.
//!
//! `cargo run -p easyrazer-licenses -- <out>`, with `cargo-about` installed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use serde::Deserialize;

const RULE: &str = "================================================================================";

/// Bundled fonts: the notice title and their license file.
const FONTS: [(&str, &str); 2] = [("Inter", "crates/ui/assets/fonts/OFL.txt"), ("Noto Sans Symbols 2, Noto Sans Math", "crates/ui/assets/fonts/NotoOFL.txt")];

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    manifest_path: String,
}

#[derive(Deserialize)]
struct User {
    #[serde(rename = "crate")]
    package: Package,
}

#[derive(Deserialize)]
struct License {
    id: String,
    name: String,
    text: String,
    used_by: Vec<User>,
}

#[derive(Deserialize)]
struct Listed {
    package: Package,
}

#[derive(Deserialize)]
struct About {
    licenses: Vec<License>,
    crates: Vec<Listed>,
}

/// Components whose copies of a license name the same holders.
struct Users {
    components: Vec<String>,
    holders: Vec<String>,
}

struct Group {
    name: String,
    users: Vec<Users>,
    text: String,
}

struct Notice {
    title: String,
    text: String,
}

/// Holders are named with a year or a sign; license terms also start lines with "copyright" or "(c)".
fn holder(line: &str) -> bool {
    let l = line.trim_start().to_lowercase();
    let digit = |s: &str| s.trim_start().starts_with(|c: char| c.is_ascii_digit());
    if let Some(rest) = l.strip_prefix("copyright") {
        let rest = rest.trim_start();
        return rest.starts_with("(c)") || rest.starts_with('\u{a9}') || digit(rest);
    }
    l.strip_prefix("(c)").or_else(|| l.strip_prefix('\u{a9}')).is_some_and(digit)
}

fn holders(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    text.lines().filter(|l| holder(l)).map(|l| l.trim().to_string()).filter(|l| seen.insert(l.clone())).collect()
}

fn wording(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Copies of one license differ in holders, whitespace and headers, so the most common wording stands for all.
fn common(texts: &[&str]) -> String {
    let bodies: Vec<String> = texts.iter().map(|t| t.lines().filter(|l| !holder(l)).collect::<Vec<_>>().join("\n").trim().to_string()).collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for b in &bodies {
        *counts.entry(wording(b)).or_default() += 1;
    }
    let best = bodies.iter().map(|b| counts[&wording(b)]).max().unwrap_or(0);
    bodies.into_iter().find(|b| counts[&wording(b)] == best).unwrap_or_default()
}

fn is_notice(name: &str) -> bool {
    let name = name.to_lowercase().replace('_', "-");
    name.starts_with("notice") || name.starts_with("third-party-notices")
}

fn notice_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()) && is_notice(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}

fn cargo_about(root: &Path) -> About {
    let out = env::temp_dir().join(format!("easyrazer-about-{}.json", std::process::id()));
    // cargo-about refuses to write JSON to a pipe when it detects PowerShell, so it goes to a file.
    let status = Command::new("cargo")
        .args(["about", "generate", "--format", "json", "-c"])
        .arg(root.join("about.toml"))
        .arg("-m")
        .arg(root.join("crates/ui/Cargo.toml"))
        .arg("-o")
        .arg(&out)
        .current_dir(root)
        .status()
        .expect("cargo about");
    assert!(status.success(), "cargo about failed");
    let text = fs::read_to_string(&out).expect("cargo about output");
    let _ = fs::remove_file(&out);
    serde_json::from_str(&text).expect("cargo about JSON")
}

fn normalized(p: &str) -> String {
    p.to_lowercase().replace('/', "\\")
}

fn groups(about: &About, third_party: &dyn Fn(&Package) -> bool) -> Vec<Group> {
    let ids: BTreeSet<&str> = about.licenses.iter().map(|l| l.id.as_str()).collect();
    let mut groups: Vec<Group> = ids
        .into_iter()
        .filter_map(|id| {
            let copies: Vec<&License> = about.licenses.iter().filter(|l| l.id == id).collect();
            let users: Vec<Users> = copies
                .iter()
                .filter_map(|l| {
                    let components: Vec<String> = l.used_by.iter().filter(|u| third_party(&u.package)).map(|u| format!("{} {}", u.package.name, u.package.version)).collect();
                    (!components.is_empty()).then(|| Users { components, holders: holders(&l.text) })
                })
                .collect();
            let texts: Vec<&str> = copies.iter().map(|l| l.text.as_str()).collect();
            (!users.is_empty()).then(|| Group { name: copies[0].name.clone(), users, text: common(&texts) })
        })
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.users.iter().map(|u| u.components.len()).sum::<usize>()));
    groups
}

fn notices(about: &About, root: &Path, third_party: &dyn Fn(&Package) -> bool) -> Vec<Notice> {
    let mut notices: Vec<Notice> = about
        .crates
        .iter()
        .filter(|c| third_party(&c.package))
        .flat_map(|c| {
            let dir = Path::new(&c.package.manifest_path).parent().map(Path::to_path_buf).unwrap_or_default();
            let title = format!("{} {}", c.package.name, c.package.version);
            notice_files(&dir).into_iter().map(move |f| Notice { title: title.clone(), text: fs::read_to_string(f).unwrap_or_default().trim().to_string() })
        })
        .collect();
    notices.extend(FONTS.iter().map(|(title, path)| Notice { title: (*title).to_string(), text: fs::read_to_string(root.join(path)).expect("font license").trim().to_string() }));
    notices
}

fn section(title: &str, body: &str) -> String {
    format!("{RULE}\n{title}\n{RULE}\n\n{}\n\n", body.trim())
}

/// EasyRazer's license, the components by license, then each text once.
fn text(own: &str, groups: &[Group], notices: &[Notice]) -> String {
    let names = |g: &Group| {
        let mut seen = BTreeSet::new();
        g.users.iter().flat_map(|u| u.components.iter()).filter(|c| seen.insert(c.as_str())).cloned().collect::<Vec<_>>().join(", ")
    };
    let list = groups.iter().map(|g| format!("{}:\n  {}", g.name, names(g)));
    let listed = notices.iter().map(|n| format!("Notice: {}", n.title));
    let mut out = section("EasyRazer", own) + &section("Third-party components", &list.chain(listed).collect::<Vec<_>>().join("\n\n"));
    for g in groups {
        let users = g.users.iter().map(|u| std::iter::once(u.components.join(", ")).chain(u.holders.iter().map(|h| format!("  {h}"))).collect::<Vec<_>>().join("\n"));
        out += &section(&g.name, &users.chain([String::new(), g.text.clone()]).collect::<Vec<_>>().join("\n"));
    }
    for n in notices {
        out += &section(&n.title, &n.text);
    }
    out
}

fn main() {
    let out = env::args().nth(1).expect("usage: easyrazer-licenses <out>");
    let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).canonicalize().expect("repository root");
    // `canonicalize` gives a verbatim path on Windows; cargo prints plain ones.
    let root = PathBuf::from(root.to_string_lossy().trim_start_matches(r"\\?\").to_string());
    let about = cargo_about(&root);
    let ours = normalized(&root.to_string_lossy());
    let third_party = |p: &Package| !normalized(&p.manifest_path).starts_with(&ours);
    let groups = groups(&about, &third_party);
    let notices = notices(&about, &root, &third_party);
    let own = fs::read_to_string(root.join("LICENSE")).expect("LICENSE");
    fs::write(&out, text(&own, &groups, &notices)).expect("write");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holders_are_lines_with_a_year_or_a_sign() {
        assert!(holder("Copyright (c) 2016 The Inter Project Authors"));
        assert!(holder("  Copyright 2022 Someone"));
        assert!(holder("(c) 2020 Someone"));
        assert!(!holder("Copyright notice and this permission notice shall be included"));
        assert!(!holder("(c) You must give any other recipients"));
    }

    #[test]
    fn the_most_common_wording_stands_for_all_copies() {
        let texts = ["Copyright 2020 A\nSame  terms", "Copyright 2021 B\nSame terms", "Other terms"];
        assert_eq!(common(&texts), "Same  terms");
    }

    #[test]
    fn notice_files_go_by_name() {
        assert!(is_notice("NOTICE"));
        assert!(is_notice("third_party_notices.txt"));
        assert!(!is_notice("LICENSE-MIT"));
    }
}
