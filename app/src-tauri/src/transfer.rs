//! Profile files: one profile with the macros it plays, portable between keyboards.

use std::collections::{BTreeMap, BTreeSet};

use razer_core::binding::{self, Action};
use razer_core::devices;
use razer_core::layout;
use razer_core::lighting::Rgb;
use razer_core::macros::{self, Event};
use razer_core::profiles::Snapshot;
use serde::{Deserialize, Serialize};

use crate::device::next_macro_id;
use crate::profiles;
use crate::settings::{DeviceData, Macro, Profile, Rapid, SnapTap};

/// 2: layers as factory changes, grouped values; 1 still reads.
const FORMAT: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileFile {
    pub format: u32,
    /// Hex, the way Windows shows it.
    pub pid: String,
    pub model: String,
    pub profile: Exported,
    pub macros: BTreeMap<u16, FileMacro>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Exported {
    pub name: String,
    #[serde(with = "crate::stored::snapshot")]
    pub data: Snapshot,
    #[serde(with = "crate::stored::grouped")]
    pub rapid: BTreeMap<u8, Rapid>,
    pub snap_tap: SnapTap,
    #[serde(default)]
    pub continuous: bool,
    #[serde(default, with = "crate::stored::colors")]
    pub custom: Option<BTreeMap<u8, Rgb>>,
}

/// A macro as the file holds it: what is flash state on one keyboard does not travel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileMacro {
    pub name: String,
    pub events: Vec<Event>,
}

#[derive(Debug)]
pub enum Refused {
    Format,
    Broken(String),
    UnknownModel(String),
}

#[cfg_attr(not(test), allow(dead_code))]
pub struct Imported {
    pub id: u32,
    pub name: String,
    /// Keys the target keyboard does not have, counted once across everything the profile keeps per key.
    pub dropped: usize,
}

fn macro_ids(s: &Snapshot) -> impl Iterator<Item = u16> + '_ {
    [&s.normal, &s.hypershift].into_iter().flat_map(|l| l.values()).filter_map(|r| match binding::decode(r.fn_id, &r.fn_data) {
        Some(Action::Macro { id, .. }) => Some(id),
        _ => None,
    })
}

pub fn export(d: &DeviceData, profile: u32) -> Option<ProfileFile> {
    let p = d.profile(profile)?;
    let macros = macro_ids(&p.data)
        .filter_map(|id| d.macros.get(&id).map(|m| (id, FileMacro { name: m.name.clone(), events: m.events.clone() })))
        .collect();
    Some(ProfileFile {
        format: FORMAT,
        pid: format!("{:04X}", d.pid),
        model: devices::by_pid(d.pid).map(|s| s.name.clone()).unwrap_or_default(),
        profile: Exported { name: p.name.clone(), data: p.data.clone(), rapid: p.rapid.clone(), snap_tap: p.snap_tap.clone(), continuous: p.continuous, custom: p.custom.clone() },
        macros,
    })
}

pub fn parse(text: &str) -> Result<ProfileFile, Refused> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| Refused::Broken(e.to_string()))?;
    if v.get("format").and_then(|f| f.as_u64()).is_some_and(|f| f > FORMAT.into()) {
        return Err(Refused::Format);
    }
    let f: ProfileFile = serde_json::from_value(v).map_err(|e| Refused::Broken(e.to_string()))?;
    if !(1..=FORMAT).contains(&f.format) {
        return Err(Refused::Broken(format!("format {}", f.format)));
    }
    Ok(f)
}

/// Changes `d` only once the whole profile is converted.
pub fn import(d: &mut DeviceData, f: ProfileFile) -> Result<Imported, Refused> {
    let unknown = || Refused::UnknownModel(f.pid.clone());
    let source = u16::from_str_radix(&f.pid, 16).ok().ok_or_else(unknown)?;
    let from = devices::by_pid(source).ok_or_else(unknown)?;
    let to = devices::by_pid(d.pid).ok_or_else(unknown)?;

    if f.macros.values().any(|m| macros::encode(&m.events).is_none()) {
        return Err(Refused::Broken("a macro has a key the keyboard cannot play".into()));
    }

    let mut library = d.macros.clone();
    let mut ids = BTreeMap::new();
    for (old, m) in f.macros {
        if library.contains_key(&u16::MAX) {
            return Err(Refused::Broken("too many macros".into()));
        }
        let new = next_macro_id(&library);
        library.insert(new, Macro { name: m.name, events: m.events, written: false });
        ids.insert(old, new);
    }

    let keys: BTreeSet<u8> = layout::keys(to.layout).into_iter().filter(|k| k.editable).map(|k| k.key).collect();
    let mut dropped = BTreeSet::new();
    let mut keep = |k: u8| {
        let has = keys.contains(&k);
        if !has {
            dropped.insert(k);
        }
        has
    };
    let Exported { name, mut data, mut rapid, mut snap_tap, continuous, mut custom } = f.profile;

    let rescale = source != d.pid;
    let [lo, hi] = to.analog.range.map(|mm| to.analog.threshold(mm));
    // The same scale needs no round trip through mm, which could shift in-range values.
    let convert = |t: u8| if rescale { to.analog.threshold(from.analog.mm(t).clamp(to.analog.range[0], to.analog.range[1])) } else { t.clamp(lo, hi) };
    for layer in [&mut data.normal, &mut data.hypershift] {
        layer.retain(|&k, _| keep(k));
        for (&k, r) in layer.iter_mut() {
            r.thr_low = convert(r.thr_low);
            // Opaque to the app, which writes only thr_low; 0 means unset.
            if r.thr_high != 0 {
                r.thr_high = convert(r.thr_high);
            }
            if let Some(Action::Macro { id, mode, count }) = binding::decode(r.fn_id, &r.fn_data) {
                let action = ids.get(&id).map(|&id| Action::Macro { id, mode, count }).unwrap_or_else(|| binding::factory(k));
                (r.fn_id, r.fn_data) = binding::encode(action).unwrap_or((0, Vec::new()));
            }
        }
    }
    rapid.retain(|&k, _| keep(k));
    for g in &mut snap_tap.groups {
        g.keys.retain(|&k| keep(k));
    }
    snap_tap.groups.retain(|g| g.keys.len() >= 2);
    if let Some(c) = &mut custom {
        c.retain(|&k, _| keep(k));
    }
    if data.look.as_ref().is_some_and(|l| !to.lighting.effects.contains(&l.effect.name)) {
        data.look = None;
    }

    let name = profiles::free_name(d, &name, |n| if n == 1 { String::new() } else { format!(" ({n})") });
    let id = d.new_profile_id();
    d.profiles.push(Profile { id, name: name.clone(), slot: None, data, rapid, snap_tap, continuous, custom });
    d.macros = library;
    Ok(Imported { id, name, dropped: dropped.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::binding::{self, Action, MacroMode};
    use razer_core::devices;
    use razer_core::layout::{self, Board};
    use razer_core::lighting::{Effect, Look};
    use razer_core::profiles::RawKey;
    use razer_core::rapid::SnapGroup;

    use crate::settings::{DeviceData, Macro, Profile, Rapid};

    fn key(thr: u8, a: Option<Action>) -> RawKey {
        let (fn_id, fn_data) = a.and_then(binding::encode).unwrap_or((0, Vec::new()));
        RawKey { thr_low: thr, thr_high: thr, fn_id, fn_data }
    }
    fn play(id: u16) -> Option<Action> { Some(Action::Macro { id, mode: MacroMode::Times, count: 1 }) }
    fn board(pid: u16) -> DeviceData { DeviceData { pid, ..Default::default() } }
    fn with_profile(d: &mut DeviceData, name: &str, normal: &[(u8, RawKey)], hs: &[(u8, RawKey)]) -> u32 {
        let id = d.new_profile_id();
        let data = Snapshot { normal: normal.iter().cloned().collect(), hypershift: hs.iter().cloned().collect(), look: None };
        d.profiles.push(Profile { id, name: name.into(), data, ..Default::default() });
        id
    }
    fn mac(name: &str) -> Macro { Macro { name: name.into(), events: Vec::new(), written: true } }

    #[test]
    fn export_takes_the_profile_and_only_its_macros() {
        let mut d = board(0x0266);
        d.macros.insert(0x8000, mac("used"));
        d.macros.insert(0x8001, mac("other"));
        let p = with_profile(&mut d, "Game", &[(1, key(100, play(0x8000)))], &[]);
        let f = export(&d, p).unwrap();
        assert_eq!((f.format, f.pid.as_str(), f.profile.name.as_str()), (FORMAT, "0266", "Game"));
        assert_eq!(f.macros.keys().copied().collect::<Vec<_>>(), [0x8000]);
        let json = serde_json::to_string(&f).unwrap();
        assert!(!json.contains("\"slot\"") && !json.contains("\"written\""));
    }

    #[test]
    fn import_renumbers_macros_and_rewrites_bindings() {
        let mut src = board(0x0266);
        src.macros.insert(0x8000, mac("m"));
        let p = with_profile(&mut src, "Game", &[(1, key(100, play(0x8000)))], &[(2, key(100, play(0x8000)))]);
        let f = export(&src, p).unwrap();
        let mut dst = board(0x0266);
        dst.macros.insert(0x8000, mac("theirs"));
        let got = import(&mut dst, f).unwrap();
        let new_id = *dst.macros.keys().max().unwrap();
        assert_ne!(new_id, 0x8000);
        assert!(!dst.macros[&new_id].written);
        let prof = dst.profile(got.id).unwrap();
        for layer in [&prof.data.normal, &prof.data.hypershift] {
            let a = layer.values().next().map(|r| binding::decode(r.fn_id, &r.fn_data)).unwrap();
            assert_eq!(a, play(new_id));
        }
        assert_eq!(dst.macros[&0x8000].name, "theirs");
    }

    #[test]
    fn a_binding_to_a_macro_not_in_the_file_falls_back_to_factory() {
        let mut src = board(0x0266);
        let p = with_profile(&mut src, "Game", &[(1, key(100, play(0x9999)))], &[]);
        let mut f = export(&src, p).unwrap();
        f.macros.clear();
        let mut dst = board(0x0266);
        dst.macros.insert(0x9999, mac("unrelated"));
        let got = import(&mut dst, f).unwrap();
        let r = &dst.profile(got.id).unwrap().data.normal[&1];
        assert_eq!(binding::decode(r.fn_id, &r.fn_data), Some(binding::factory(1)), "the key keeps its press point and falls back to its factory binding");
    }

    #[test]
    fn a_macro_that_cannot_be_encoded_refuses_the_import() {
        let mut src = board(0x0266);
        src.macros.insert(0x8000, mac("m"));
        let p = with_profile(&mut src, "Game", &[(1, key(100, play(0x8000)))], &[]);
        let mut f = export(&src, p).unwrap();
        f.macros.get_mut(&0x8000).unwrap().events = vec![Event::Key { key: 255, down: true }];
        let mut dst = board(0x0266);
        let before = dst.clone();
        assert!(matches!(import(&mut dst, f), Err(Refused::Broken(_))));
        assert_eq!(dst, before);
    }

    #[test]
    fn only_a_newer_numeric_format_is_a_format_refusal() {
        let file = |format: &str| format!(r#"{{"format":{format},"pid":"0266","model":"x","profile":{{"name":"a","data":{{"normal":{{}},"hypershift":{{}},"look":null}},"rapid":{{}},"snap_tap":{{}},"custom":null}},"macros":{{}}}}"#);
        assert!(matches!(parse(&file("3")), Err(Refused::Format)));
        for bad in ["null", r#""1""#, "0", "true"] {
            assert!(matches!(parse(&file(bad)), Err(Refused::Broken(_))), "{bad}");
        }
        assert!(parse(&file("1")).is_ok() && parse(&file("2")).is_ok());
    }

    #[test]
    fn a_threshold_outside_the_range_is_clamped_on_the_same_model() {
        let spec = devices::by_pid(0x02E6).unwrap();
        let [lo, hi] = spec.analog.range.map(|mm| spec.analog.threshold(mm));
        let inside = (lo + hi) / 2;
        let mut src = board(0x02E6);
        let p = with_profile(&mut src, "Game", &[(1, key(255, None)), (2, key(0, None)), (3, key(inside, None))], &[]);
        let mut dst = board(0x02E6);
        let got = import(&mut dst, export(&src, p).unwrap()).unwrap();
        let n = &dst.profile(got.id).unwrap().data.normal;
        assert_eq!((n[&1].thr_low, n[&2].thr_low, n[&3].thr_low), (hi, lo, inside));
    }

    #[test]
    fn import_takes_a_free_name_and_never_loads() {
        let mut d = board(0x0266);
        let p = with_profile(&mut d, "Game", &[], &[]);
        d.loaded = Some(p);
        let f = export(&d, p).unwrap();
        let got = import(&mut d, f).unwrap();
        assert_eq!(got.name, "Game (2)");
        assert_eq!(d.loaded, Some(p));
        assert_eq!(d.profile(got.id).unwrap().slot, None);
    }

    #[test]
    #[allow(non_snake_case)]
    fn import_from_another_model_maps_and_clamps() {
        // The V3 Pro Low-profile TKL: no numpad, another scale, press points only up to 2.8 mm.
        const TARGET: u16 = 0x02E6;
        let fw = |label: &str| layout::keys(Board::Full).into_iter().find(|k| k.label == label).unwrap().key;
        let (KEPT, MISSING) = (fw("Esc"), fw("Num"));
        let src_spec = devices::by_pid(0x0266).unwrap();
        let dst_spec = devices::by_pid(TARGET).unwrap();
        let mut src = board(0x0266);
        let deep = src_spec.analog.threshold(src_spec.analog.range[1]);
        let p = with_profile(&mut src, "Game", &[(KEPT, key(deep, None)), (MISSING, key(100, None))], &[(MISSING, key(100, None))]);
        src.profiles[0].rapid.insert(MISSING, Rapid { enabled: true, press: 0.3, release: 0.3, continuous: false });
        src.profiles[0].custom = Some([(MISSING, [1, 2, 3]), (KEPT, [4, 5, 6])].into());
        let f = export(&src, p).unwrap();
        let mut dst = board(TARGET);
        let got = import(&mut dst, f).unwrap();
        let prof = dst.profile(got.id).unwrap();
        assert!(!prof.data.normal.contains_key(&MISSING) && !prof.data.hypershift.contains_key(&MISSING));
        assert!(!prof.rapid.contains_key(&MISSING));
        assert_eq!(prof.custom.as_ref().unwrap().keys().copied().collect::<Vec<_>>(), [KEPT]);
        let mm = dst_spec.analog.mm(prof.data.normal[&KEPT].thr_low);
        assert!(mm <= dst_spec.analog.range[1] + 0.05, "clamped to the target's range");
        assert_eq!(got.dropped, 1);
    }

    #[test]
    fn an_effect_the_target_lacks_is_dropped() {
        let look = |name: &str| Some(Look { effect: Effect { name: name.into(), ..Default::default() }, brightness: 100 });
        let kept = devices::by_pid(0x02E6).unwrap().lighting.effects[0].clone();
        for (name, survives) in [(kept.as_str(), true), ("no-such-effect", false), ("custom", false)] {
            let mut src = board(0x0266);
            let p = with_profile(&mut src, "Game", &[], &[]);
            src.profiles[0].data.look = look(name);
            let mut dst = board(0x02E6);
            let got = import(&mut dst, export(&src, p).unwrap()).unwrap();
            assert_eq!(dst.profile(got.id).unwrap().data.look.is_some(), survives, "{name}");
        }
    }

    #[test]
    fn a_press_point_below_the_targets_range_is_raised() {
        let fw = layout::keys(Board::Full).into_iter().find(|k| k.label == "Esc").unwrap().key;
        let (from, to) = (devices::by_pid(0x02A6).unwrap(), devices::by_pid(0x0266).unwrap());
        let mut src = board(0x02A6);
        let p = with_profile(&mut src, "Game", &[(fw, key(from.analog.threshold(from.analog.range[0]), None))], &[]);
        let mut dst = board(0x0266);
        let got = import(&mut dst, export(&src, p).unwrap()).unwrap();
        let mm = to.analog.mm(dst.profile(got.id).unwrap().data.normal[&fw].thr_low);
        assert!(mm >= to.analog.range[0] - 0.05, "{mm}");
    }

    #[test]
    fn snap_groups_lose_missing_keys() {
        let fw = |label: &str| layout::keys(Board::Full).into_iter().find(|k| k.label == label).unwrap().key;
        let (a, b, num) = (fw("Esc"), fw("Tab"), fw("Num"));
        let mut src = board(0x0266);
        let p = with_profile(&mut src, "Game", &[], &[]);
        let group = |keys: &[u8]| SnapGroup { keys: keys.to_vec(), rule: Default::default() };
        src.profiles[0].snap_tap.groups = vec![group(&[a, b, num]), group(&[a, num])];
        let mut dst = board(0x02E6);
        let got = import(&mut dst, export(&src, p).unwrap()).unwrap();
        assert_eq!(dst.profile(got.id).unwrap().snap_tap.groups, vec![group(&[a, b])]);
    }

    #[test]
    fn too_many_macros_are_refused() {
        let mut dst = board(0x0266);
        dst.macros.insert(u16::MAX, mac("last"));
        let mut src = board(0x0266);
        src.macros.insert(0x8000, mac("m"));
        let p = with_profile(&mut src, "Game", &[(1, key(100, play(0x8000)))], &[]);
        let before = dst.clone();
        assert!(matches!(import(&mut dst, export(&src, p).unwrap()), Err(Refused::Broken(_))));
        assert_eq!(dst, before);
    }

    #[test]
    fn bad_files_are_refused() {
        assert!(matches!(parse("not json"), Err(Refused::Broken(_))));
        assert!(matches!(parse(r#"{"format":3,"pid":"0266","model":"x","profile":{"name":"a","data":{"normal":{},"hypershift":{},"look":null},"rapid":{},"snap_tap":{},"custom":null},"macros":{}}"#), Err(Refused::Format)));
        let mut d = board(0x0266);
        let before = d.clone();
        let f = parse(r#"{"format":1,"pid":"FFFF","model":"x","profile":{"name":"a","data":{"normal":{},"hypershift":{},"look":null},"rapid":{},"snap_tap":{},"custom":null},"macros":{}}"#).unwrap();
        assert!(matches!(import(&mut d, f), Err(Refused::UnknownModel(_))));
        assert_eq!(d, before, "a refused import changes nothing");
    }
}
