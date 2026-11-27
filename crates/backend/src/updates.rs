//! Whether GitHub has a release newer than this build.

use serde::Deserialize;

const HOST: &str = "api.github.com";
/// Pre-releases, the nightly among them, are never the latest release.
const PATH: &str = "/repos/janeblower/easyrazer/releases/latest";
const LIMIT: usize = 512 * 1024;

#[derive(Deserialize)]
struct Latest {
    tag_name: String,
}

/// The numbers of `1.2.3`; `None` for anything else, such as a branch name.
fn numbers(version: &str) -> Option<Vec<u64>> {
    version.trim().trim_start_matches('v').split('.').map(|p| p.parse().ok()).collect()
}

/// A release build: `0.0.0` marks a local or nightly build, which has no place among releases.
pub fn is_release(version: &str) -> bool {
    numbers(version).is_some_and(|n| n.iter().any(|&x| x > 0))
}

/// `candidate` is a later version than `current`; `1.2` and `1.2.0` are the same.
pub fn newer(candidate: &str, current: &str) -> bool {
    let (Some(mut a), Some(mut b)) = (numbers(candidate), numbers(current)) else { return false };
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

/// The tag of the latest release.
pub fn latest() -> Result<String, String> {
    let body = crate::sys::download(HOST, PATH, LIMIT).map_err(|e| e.to_string())?;
    let latest: Latest = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if numbers(&latest.tag_name).is_none() {
        return Err(format!("unexpected release tag {}", latest.tag_name));
    }
    Ok(latest.tag_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(newer("0.10.0", "0.9.1"));
        assert!(newer("1.0", "0.99.99"));
        assert!(!newer("0.9.1", "0.9.1"));
        assert!(!newer("0.9", "0.9.0"));
        assert!(!newer("0.9.0", "0.10.0"));
        assert!(!newer("nightly", "0.9.0"));
    }

    #[test]
    fn local_and_nightly_builds_are_not_releases() {
        assert!(!is_release("0.0.0"));
        assert!(!is_release("dev"));
        assert!(is_release("0.10.0"));
    }
}
