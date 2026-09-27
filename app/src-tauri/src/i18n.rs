//! Interface texts, shared with the window: `app/src/locales/*.json`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

const LOCALES: &[(&str, &str)] = &[("ru", include_str!("../../src/locales/ru.json")), ("en", include_str!("../../src/locales/en.json"))];
const FALLBACK: &str = "en";

fn tables() -> &'static BTreeMap<&'static str, Value> {
    static ALL: OnceLock<BTreeMap<&'static str, Value>> = OnceLock::new();
    ALL.get_or_init(|| LOCALES.iter().map(|(l, s)| (*l, serde_json::from_str(s).expect("embedded locale"))).collect())
}

fn lookup(lang: &str, key: &str) -> Option<&'static str> {
    key.split('.').try_fold(tables().get(lang)?, |v, k| v.get(k))?.as_str()
}

pub fn known(lang: &str) -> bool {
    tables().contains_key(lang)
}

/// Text for a dotted `key`; an unknown language or key falls back to English, then to the key.
pub fn t(lang: &str, key: &str) -> String {
    lookup(lang, key).or_else(|| lookup(FALLBACK, key)).unwrap_or(key).to_string()
}

/// `t` with `{name}` placeholders filled in, the same syntax the window uses.
pub fn tf(lang: &str, key: &str, args: &[(&str, &str)]) -> String {
    args.iter().fold(t(lang, key), |s, (name, value)| s.replace(&format!("{{{name}}}"), value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(v: &Value, prefix: &str, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => m.iter().for_each(|(k, v)| keys(v, &format!("{prefix}{k}."), out)),
            _ => out.push(prefix.trim_end_matches('.').to_string()),
        }
    }

    #[test]
    fn every_locale_has_the_same_keys() {
        let all: Vec<Vec<String>> = tables()
            .values()
            .map(|v| {
                let mut out = Vec::new();
                keys(v, "", &mut out);
                out.sort();
                out
            })
            .collect();
        assert!(all.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn looks_up_with_fallbacks() {
        assert_eq!(t("ru", "tray.quit"), "Выход");
        assert_eq!(t("en", "tray.quit"), "Quit");
        assert_eq!(t("de", "tray.quit"), "Quit");
        assert_eq!(t("ru", "no.such.key"), "no.such.key");
        assert_eq!(t("ru", "tray"), "tray");
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(tf("en", "backend.autostart", &[("error", "denied")]), "Autostart: denied");
    }
}
