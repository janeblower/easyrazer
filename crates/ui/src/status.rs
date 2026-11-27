//! The status line under the tabs: what the app is doing with which keyboard.

use easyrazer_backend::device::Status;
use easyrazer_backend::i18n;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Info,
    Warn,
    Ok,
}

/// Everything the line can speak of, most urgent first in [`line`].
pub struct Input<'a> {
    pub status: Option<&'a Status>,
    pub progress: Option<(usize, usize)>,
    /// PID of a Razer keyboard without a description.
    pub unsupported: Option<u16>,
    /// The user's name for the keyboard.
    pub name: Option<&'a str>,
    pub message: &'a str,
    pub error: &'a str,
}

pub fn line(lang: &str, i: &Input) -> (Kind, String) {
    let t = |key: &str| i18n::t(lang, key);
    let tf = |key: &str, args: &[(&str, &str)]| i18n::tf(lang, key, args);
    if !i.error.is_empty() {
        return (Kind::Warn, i.error.to_string());
    }
    if let Some((done, total)) = i.progress {
        let percent = (done * 100).checked_div(total).unwrap_or(0).to_string();
        return (Kind::Info, tf("status.reading", &[("percent", &percent)]));
    }
    let Some(s) = i.status else { return (Kind::Info, t("status.searching")) };
    if s.synapse {
        return (Kind::Warn, t("status.synapse"));
    }
    if !i.message.is_empty() {
        return (Kind::Info, i.message.to_string());
    }
    let model = s.model.as_deref().unwrap_or("");
    if !s.device {
        if let Some(m) = &s.model {
            return (Kind::Warn, tf("status.offline", &[("name", i.name.unwrap_or(m))]));
        }
        if let Some(pid) = i.unsupported {
            return (Kind::Warn, tf("status.unsupported", &[("pid", &format!("{pid:04X}"))]));
        }
        return (Kind::Warn, t("status.notFound"));
    }
    let profile = s.profile_name.as_deref().unwrap_or("?");
    if !s.verified {
        return (Kind::Warn, tf("status.unverified", &[("model", model), ("profile", profile)]));
    }
    (Kind::Ok, tf("status.connected", &[("model", model), ("profile", profile)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(device: bool, model: Option<&str>) -> Status {
        Status {
            id: Some("k".into()),
            device,
            synapse: false,
            driver_mode: false,
            profile: Some(1),
            profile_name: Some("Games".into()),
            model: model.map(Into::into),
            verified: true,
            analog: None,
            error: None,
        }
    }

    fn input(status: Option<&Status>) -> Input<'_> {
        Input { status, progress: None, unsupported: None, name: None, message: "", error: "" }
    }

    #[test]
    fn an_error_wins_over_everything() {
        let s = status(true, Some("Kb"));
        let i = Input { error: "boom", progress: Some((1, 2)), ..input(Some(&s)) };
        assert_eq!(line("en", &i), (Kind::Warn, "boom".into()));
    }

    #[test]
    fn a_read_shows_whole_percents() {
        let i = Input { progress: Some((2, 3)), ..input(None) };
        assert_eq!(line("en", &i), (Kind::Info, "Reading the keyboard: 66%".into()));
    }

    #[test]
    fn a_connected_keyboard_names_its_profile() {
        let s = status(true, Some("Kb"));
        let (kind, text) = line("en", &input(Some(&s)));
        assert_eq!(kind, Kind::Ok);
        assert!(text.contains("Kb") && text.contains("Games"));
    }

    #[test]
    fn an_offline_keyboard_goes_by_the_users_name() {
        let s = status(false, Some("Kb"));
        let i = Input { name: Some("Desk"), ..input(Some(&s)) };
        let (kind, text) = line("en", &i);
        assert_eq!(kind, Kind::Warn);
        assert!(text.starts_with("Desk "));
    }

    #[test]
    fn an_unknown_model_shows_its_pid() {
        let s = status(false, None);
        let i = Input { unsupported: Some(0x2a5), ..input(Some(&s)) };
        assert!(line("en", &i).1.contains("PID 02A5"));
    }

    #[test]
    fn synapse_hides_the_connection() {
        let mut s = status(true, Some("Kb"));
        s.synapse = true;
        assert!(line("ru", &input(Some(&s))).1.contains("Synapse"));
    }
}
