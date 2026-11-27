//! The lighting controls and the firmware effect they stand for.
//!
//! Effect names are `group[_type][_one|_two|_random]`; the window offers groups and types and
//! picks the variant from the colors set.

use std::collections::BTreeMap;

use razer_core::lighting::{Effect, EffectInfo, Look, Rgb};

pub const DEFAULT_COLORS: [Rgb; 2] = [[0x44, 0xd6, 0x2c], [0x00, 0x40, 0xff]];
const WHITE: Rgb = [0xff; 3];
pub const BLACK: Rgb = [0; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    One,
    Two,
    Random,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub group: String,
    pub kind: Option<String>,
    pub variant: Option<Variant>,
}

pub fn parse(name: &str) -> Parsed {
    let mut parts: Vec<&str> = name.split('_').collect();
    let variant = match parts.last() {
        Some(&"one") => Some(Variant::One),
        Some(&"two") => Some(Variant::Two),
        Some(&"random") => Some(Variant::Random),
        _ => None,
    };
    if variant.is_some() {
        parts.pop();
    }
    Parsed { group: parts[0].to_string(), kind: parts.get(1).map(|s| s.to_string()), variant }
}

pub struct Group {
    pub group: String,
    pub types: Vec<String>,
    pub effects: Vec<(EffectInfo, Parsed)>,
}

pub fn groups(effects: &[EffectInfo]) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for e in effects {
        let p = parse(&e.name);
        let at = match out.iter().position(|g| g.group == p.group) {
            Some(i) => i,
            None => {
                out.push(Group { group: p.group.clone(), types: Vec::new(), effects: Vec::new() });
                out.len() - 1
            }
        };
        let g = &mut out[at];
        if let Some(t) = &p.kind
            && !g.types.contains(t)
        {
            g.types.push(t.clone());
        }
        g.effects.push((e.clone(), p));
    }
    out
}

/// What the controls show; the effect sent to the keyboard is derived from it.
#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    pub group: String,
    pub kind: Option<String>,
    pub colors: Vec<Option<Rgb>>,
    pub random: bool,
    pub dir: Option<String>,
    pub speed: Option<u8>,
    pub brightness: u8,
    pub paint: BTreeMap<u8, Rgb>,
}

pub fn candidates<'a>(groups: &'a [Group], c: &Controls) -> Vec<&'a (EffectInfo, Parsed)> {
    groups.iter().find(|g| g.group == c.group).map(|g| g.effects.iter().filter(|(_, p)| p.kind == c.kind).collect()).unwrap_or_default()
}

/// One set color picks the one-color effect, two pick the two-color one, none turn the lighting off.
pub fn resolve(groups: &[Group], c: &Controls) -> Option<Look> {
    let cs = candidates(groups, c);
    if cs.is_empty() {
        return None;
    }
    if c.group == "custom" {
        return Some(Look { effect: Effect { name: "custom".into(), colors: Some(c.paint.clone()), ..Default::default() }, brightness: c.brightness });
    }
    let slots = cs.iter().map(|(e, _)| e.colors).max().unwrap_or(0);
    let set: Vec<Rgb> = c.colors.iter().take(slots).flatten().copied().collect();
    let mut pick = cs[0];
    let random = cs.iter().find(|(_, p)| p.variant == Some(Variant::Random));
    if let (true, Some(r)) = (c.random, random) {
        pick = r;
    } else if cs.iter().any(|(e, _)| e.colors > 0) {
        if set.is_empty() {
            return Some(Look { effect: Effect { name: "off".into(), ..Default::default() }, brightness: c.brightness });
        }
        pick = cs.iter().find(|(e, _)| e.colors == set.len()).or_else(|| cs.iter().find(|(e, _)| e.colors == 1)).copied().unwrap_or(pick);
    }
    let e = &pick.0;
    let mut effect = Effect { name: e.name.clone(), ..Default::default() };
    if e.colors >= 1 {
        effect.rgb1 = set.first().copied();
    }
    if e.colors >= 2 {
        effect.rgb2 = set.get(1).copied();
    }
    if !e.dirs.is_empty() {
        effect.dir = Some(c.dir.clone().filter(|d| e.dirs.contains(d)).unwrap_or_else(|| e.dirs[0].clone()));
    }
    if let Some([lo, hi]) = e.speed {
        effect.speed = Some(c.speed.filter(|s| (lo..=hi).contains(s)).unwrap_or(((u16::from(lo) + u16::from(hi)) / 2) as u8));
    }
    Some(Look { effect, brightness: c.brightness })
}

pub fn blank(keys: impl Iterator<Item = u8>) -> BTreeMap<u8, Rgb> {
    keys.map(|k| (k, WHITE)).collect()
}

/// The controls showing a stored look, or the first effect when the keyboard has none the app knows.
pub fn controls_from(effects: &[EffectInfo], stored: Option<&Look>, custom: Option<&BTreeMap<u8, Rgb>>, keys: impl Iterator<Item = u8>) -> Option<Controls> {
    let look = stored.filter(|l| effects.iter().any(|e| e.name == l.effect.name));
    let paint = look.and_then(|l| l.effect.colors.clone()).or_else(|| custom.cloned()).unwrap_or_else(|| blank(keys));
    let name = look.map(|l| l.effect.name.clone()).or_else(|| effects.first().map(|e| e.name.clone()))?;
    let p = parse(&name);
    let mut c = Controls {
        group: p.group,
        kind: p.kind,
        colors: DEFAULT_COLORS.iter().copied().map(Some).collect(),
        random: p.variant == Some(Variant::Random),
        dir: None,
        speed: None,
        brightness: 255,
        paint,
    };
    if let Some(l) = look {
        let e = &l.effect;
        if let Some(rgb1) = e.rgb1 {
            c.colors = vec![Some(rgb1), e.rgb2.or(if p.variant == Some(Variant::One) { None } else { Some(DEFAULT_COLORS[1]) })];
        }
        c.dir = e.dir.clone();
        c.speed = e.speed;
        c.brightness = l.brightness;
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, colors: usize) -> EffectInfo {
        EffectInfo { name: name.into(), colors, dirs: Vec::new(), speed: None, fast_low: false }
    }

    fn effects() -> Vec<EffectInfo> {
        vec![info("off", 0), info("static", 1), info("breathing_one", 1), info("breathing_two", 2), info("breathing_random", 0), info("reactive_key_one", 1), info("reactive_ripple_one", 1)]
    }

    fn controls(group: &str, colors: Vec<Option<Rgb>>) -> Controls {
        Controls { group: group.into(), kind: None, colors, random: false, dir: None, speed: None, brightness: 255, paint: BTreeMap::new() }
    }

    #[test]
    fn names_split_into_group_type_and_variant() {
        assert_eq!(parse("reactive_ripple_one"), Parsed { group: "reactive".into(), kind: Some("ripple".into()), variant: Some(Variant::One) });
        assert_eq!(parse("static"), Parsed { group: "static".into(), kind: None, variant: None });
    }

    #[test]
    fn the_number_of_colors_picks_the_variant() {
        let g = groups(&effects());
        let one = resolve(&g, &controls("breathing", vec![Some([1, 2, 3]), None])).unwrap();
        assert_eq!(one.effect.name, "breathing_one");
        let two = resolve(&g, &controls("breathing", vec![Some([1, 2, 3]), Some([4, 5, 6])])).unwrap();
        assert_eq!((two.effect.name.as_str(), two.effect.rgb2), ("breathing_two", Some([4, 5, 6])));
        let none = resolve(&g, &controls("breathing", vec![None, None])).unwrap();
        assert_eq!(none.effect.name, "off");
        let random = resolve(&g, &Controls { random: true, ..controls("breathing", vec![None, None]) }).unwrap();
        assert_eq!(random.effect.name, "breathing_random");
    }

    #[test]
    fn a_stored_look_comes_back_as_its_controls() {
        let look = Look { effect: Effect { name: "breathing_one".into(), rgb1: Some([9, 9, 9]), ..Default::default() }, brightness: 100 };
        let c = controls_from(&effects(), Some(&look), None, std::iter::empty()).unwrap();
        assert_eq!(c.group, "breathing");
        assert_eq!(c.colors, vec![Some([9, 9, 9]), None]);
        assert_eq!(c.brightness, 100);
        assert_eq!(resolve(&groups(&effects()), &c), Some(look));
    }
}
