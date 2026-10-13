//! How profiles lie on disk: what differs from the factory, grouped, one line where it fits.

use std::collections::BTreeMap;

use razer_core::lighting::Rgb;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `keys` ascending and unique: `[1, 2, 3, 5]` → `"1-3, 5"`.
pub fn ranges(keys: &[u8]) -> String {
    let mut out = Vec::new();
    let mut i = 0;
    while i < keys.len() {
        let mut j = i;
        while j + 1 < keys.len() && u16::from(keys[j + 1]) == u16::from(keys[j]) + 1 {
            j += 1;
        }
        out.push(if i == j { keys[i].to_string() } else { format!("{}-{}", keys[i], keys[j]) });
        i = j + 1;
    }
    out.join(", ")
}

pub fn parse_ranges(s: &str) -> Result<Vec<u8>, String> {
    let mut keys = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let bad = || format!("bad key range {part:?}");
        let (a, b) = part.split_once('-').unwrap_or((part, part));
        let a: u8 = a.trim().parse().map_err(|_| bad())?;
        let b: u8 = b.trim().parse().map_err(|_| bad())?;
        if a > b {
            return Err(bad());
        }
        keys.extend(a..=b);
    }
    Ok(keys)
}

/// Keys with equal values, in the order of their first key.
fn groups<T: PartialEq>(m: &BTreeMap<u8, T>) -> Vec<(Vec<u8>, &T)> {
    let mut out: Vec<(Vec<u8>, &T)> = Vec::new();
    for (&k, v) in m {
        match out.iter_mut().find(|(_, g)| *g == v) {
            Some((keys, _)) => keys.push(k),
            None => out.push((vec![k], v)),
        }
    }
    out
}

fn expand<T: Clone, E: serde::de::Error>(raw: BTreeMap<String, T>) -> Result<BTreeMap<u8, T>, E> {
    let mut out = BTreeMap::new();
    for (r, v) in raw {
        for k in parse_ranges(&r).map_err(E::custom)? {
            out.insert(k, v.clone());
        }
    }
    Ok(out)
}

/// A map by key with equal values under one key range.
pub mod grouped {
    use super::*;

    pub fn serialize<T: Serialize + PartialEq, S: Serializer>(m: &BTreeMap<u8, T>, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(groups(m).into_iter().map(|(keys, v)| (ranges(&keys), v)))
    }

    pub fn deserialize<'de, T: Deserialize<'de> + Clone, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<u8, T>, D::Error> {
        expand(BTreeMap::<String, T>::deserialize(d)?)
    }
}

struct Hex<'a>(&'a BTreeMap<u8, Rgb>);

impl Serialize for Hex<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let hex = |[r, g, b]: Rgb| format!("#{r:02x}{g:02x}{b:02x}");
        s.collect_map(groups(self.0).into_iter().map(|(keys, c)| (ranges(&keys), hex(*c))))
    }
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum Color {
    Hex(String),
    Rgb(Rgb),
}

impl Color {
    fn rgb(self) -> Result<Rgb, String> {
        let s = match self {
            Color::Rgb(c) => return Ok(c),
            Color::Hex(s) => s,
        };
        let bad = || format!("bad color {s:?}");
        let h = s.strip_prefix('#').filter(|h| h.len() == 6 && h.is_ascii()).ok_or_else(bad)?;
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| bad());
        Ok([byte(0)?, byte(2)?, byte(4)?])
    }
}

/// Custom colors as `#rrggbb`, equal ones under one key range.
pub mod colors {
    use super::*;

    pub fn serialize<S: Serializer>(m: &Option<BTreeMap<u8, Rgb>>, s: S) -> Result<S::Ok, S::Error> {
        m.as_ref().map(Hex).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<BTreeMap<u8, Rgb>>, D::Error> {
        let Some(raw) = Option::<BTreeMap<String, Color>>::deserialize(d)? else { return Ok(None) };
        let m: BTreeMap<u8, Color> = expand(raw)?;
        m.into_iter().map(|(k, c)| c.rgb().map(|c| (k, c))).collect::<Result<_, _>>().map(Some).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Rapid;

    #[test]
    fn ranges_round_trip() {
        for keys in [vec![], vec![5], vec![1, 2, 3, 5], vec![1, 3, 5], vec![10, 11, 20, 21, 22]] {
            assert_eq!(parse_ranges(&ranges(&keys)).unwrap(), keys);
        }
        assert_eq!(ranges(&[1, 2, 3, 5]), "1-3, 5");
    }

    #[test]
    fn ranges_edges() {
        assert_eq!(ranges(&[0, 254, 255]), "0, 254-255");
        assert_eq!(parse_ranges("0, 254-255").unwrap(), [0, 254, 255]);
        assert_eq!(ranges(&[255]), "255");
        for bad in ["5-x", "300", "7-3", "a", "1--2"] {
            assert!(parse_ranges(bad).is_err(), "{bad}");
        }
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct G {
        #[serde(with = "grouped")]
        rapid: BTreeMap<u8, Rapid>,
        #[serde(with = "colors")]
        custom: Option<BTreeMap<u8, Rgb>>,
    }

    #[test]
    fn groups_equal_values() {
        let on = Rapid { enabled: true, press: 1.0, release: 1.0 };
        let fine = Rapid { enabled: true, press: 0.7, release: 0.7 };
        let mut rapid: BTreeMap<u8, Rapid> = (1..=103).map(|k| (k, on)).collect();
        rapid.insert(31, fine);
        let mut custom: BTreeMap<u8, Rgb> = (1..=103).map(|k| (k, [255, 64, 128])).collect();
        custom.insert(50, [0, 255, 0]);
        let g = G { rapid, custom: Some(custom) };
        let v = serde_json::to_value(&g).unwrap();
        assert_eq!(v["rapid"].as_object().unwrap().len(), 2);
        assert_eq!(v["rapid"]["31"]["press"], 0.7f32 as f64);
        assert_eq!(v["custom"]["1-49, 51-103"], "#ff4080");
        assert_eq!(v["custom"]["50"], "#00ff00");
        assert_eq!(serde_json::from_value::<G>(v).unwrap(), g);
        let none = G { rapid: BTreeMap::new(), custom: None };
        assert_eq!(serde_json::from_value::<G>(serde_json::to_value(&none).unwrap()).unwrap(), none);
    }

    #[test]
    fn reads_the_old_layout() {
        let old = r#"{"rapid": {"1": {"enabled": true, "press": 1.0, "release": 1.0}}, "custom": {"1": [255, 64, 128]}}"#;
        let g: G = serde_json::from_str(old).unwrap();
        assert_eq!(g.custom.unwrap()[&1], [255, 64, 128]);
        assert!(g.rapid[&1].enabled);
    }

    #[test]
    fn refuses_bad_colors() {
        for bad in [r##"{"rapid": {}, "custom": {"1": "#ff40"}}"##, r##"{"rapid": {}, "custom": {"1": "#gg4080"}}"##] {
            assert!(serde_json::from_str::<G>(bad).is_err(), "{bad}");
        }
    }
}
