//! Words for values: the serde names that the locale files key their texts by.

use serde::Serialize;

/// The serde name of a unit variant, such as `next_profile` for `System::NextProfile`.
pub fn tag<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// The version a release build was given, or the crate's own.
pub const VERSION: &str = match option_env!("EASYRAZER_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

#[cfg(test)]
mod tests {
    use razer_core::binding::System;

    #[test]
    fn a_variant_goes_by_its_serde_name() {
        assert_eq!(super::tag(&System::NextProfile), "next_profile");
    }
}
