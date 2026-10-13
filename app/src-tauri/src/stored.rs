//! How profiles lie on disk: what differs from the factory, grouped, one line where it fits.

use std::collections::BTreeMap;

use razer_core::binding;
use razer_core::lighting::{Look, Rgb};
use razer_core::profiles::{RawKey, Snapshot};
use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

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

/// What the flash holds for `key` out of the box; `None` for a key without a HID usage.
fn factory(key: u8) -> Option<RawKey> {
    binding::encode(binding::factory(key)).map(|(fn_id, fn_data)| RawKey { thr_low: 0, thr_high: 0, fn_id, fn_data })
}

/// The factory keys are listed, not left out: an empty layer must not read back as a factory one.
struct Layer<'a>(&'a BTreeMap<u8, RawKey>);

impl Serialize for Layer<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let (plain, changed): (Vec<_>, Vec<_>) = self.0.iter().partition(|(k, r)| factory(**k).as_ref() == Some(*r));
        let mut m = s.serialize_map(None)?;
        if !plain.is_empty() {
            m.serialize_entry("factory", &ranges(&plain.iter().map(|(k, _)| **k).collect::<Vec<_>>()))?;
        }
        for (k, r) in changed {
            m.serialize_entry(&k.to_string(), r)?;
        }
        m.end()
    }
}

fn layer(raw: BTreeMap<String, Value>) -> Result<BTreeMap<u8, RawKey>, String> {
    let mut out = BTreeMap::new();
    for (k, v) in raw {
        if k == "factory" {
            for key in parse_ranges(v.as_str().ok_or("factory is not a string")?)? {
                out.insert(key, factory(key).ok_or_else(|| format!("key {key} has no factory binding"))?);
            }
        } else {
            let key: u8 = k.parse().map_err(|_| format!("bad key {k:?}"))?;
            out.insert(key, serde_json::from_value(v).map_err(|e| e.to_string())?);
        }
    }
    Ok(out)
}

#[derive(Serialize)]
struct SnapshotOut<'a> {
    normal: Layer<'a>,
    hypershift: Layer<'a>,
    look: &'a Option<Look>,
}

impl<'a> SnapshotOut<'a> {
    fn of(s: &'a Snapshot) -> Self {
        Self { normal: Layer(&s.normal), hypershift: Layer(&s.hypershift), look: &s.look }
    }
}

#[derive(Deserialize)]
struct SnapshotIn {
    normal: BTreeMap<String, Value>,
    hypershift: BTreeMap<String, Value>,
    look: Option<Look>,
}

impl SnapshotIn {
    fn snapshot(self) -> Result<Snapshot, String> {
        Ok(Snapshot { normal: layer(self.normal)?, hypershift: layer(self.hypershift)?, look: self.look })
    }
}

/// A snapshot with each layer as its changes from the factory.
pub mod snapshot {
    use super::*;

    pub fn serialize<S: Serializer>(v: &Snapshot, s: S) -> Result<S::Ok, S::Error> {
        SnapshotOut::of(v).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Snapshot, D::Error> {
        SnapshotIn::deserialize(d)?.snapshot().map_err(D::Error::custom)
    }
}

/// The slot cache, each snapshot as in [`snapshot`].
pub mod slots {
    use super::*;

    pub fn serialize<S: Serializer>(v: &BTreeMap<u8, Snapshot>, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(v.iter().map(|(k, v)| (k, SnapshotOut::of(v))))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<u8, Snapshot>, D::Error> {
        BTreeMap::<u8, SnapshotIn>::deserialize(d)?.into_iter().map(|(k, v)| v.snapshot().map(|s| (k, s))).collect::<Result<_, _>>().map_err(D::Error::custom)
    }
}

const WIDTH: usize = 100;

/// JSON with every container that fits in [`WIDTH`] on one line.
pub fn to_text(v: &impl Serialize) -> Result<String, String> {
    let v = serde_json::to_value(v).map_err(|e| e.to_string())?;
    let mut out = String::new();
    write(&v, 0, &mut out);
    Ok(out)
}

fn inline(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("[{}]", a.iter().map(inline).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => format!("{{{}}}", o.iter().map(|(k, v)| format!("{}: {}", Value::from(k.as_str()), inline(v))).collect::<Vec<_>>().join(", ")),
        _ => v.to_string(),
    }
}

fn write(v: &Value, indent: usize, out: &mut String) {
    let line = inline(v);
    let (open, close, items): (char, char, Vec<(Option<&str>, &Value)>) = match v {
        Value::Array(a) if indent + line.len() > WIDTH => ('[', ']', a.iter().map(|x| (None, x)).collect()),
        Value::Object(o) if indent + line.len() > WIDTH => ('{', '}', o.iter().map(|(k, x)| (Some(k.as_str()), x)).collect()),
        _ => return out.push_str(&line),
    };
    out.push(open);
    for (i, (k, x)) in items.into_iter().enumerate() {
        out.push_str(if i == 0 { "\n" } else { ",\n" });
        out.push_str(&" ".repeat(indent + 2));
        if let Some(k) = k {
            out.push_str(&format!("{}: ", Value::from(k)));
        }
        write(x, indent + 2, out);
    }
    out.push('\n');
    out.push_str(&" ".repeat(indent));
    out.push(close);
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
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct P {
        #[serde(with = "snapshot")]
        data: Snapshot,
        #[serde(with = "slots")]
        slots: BTreeMap<u8, Snapshot>,
    }

    fn full() -> Snapshot {
        let layer = || -> BTreeMap<u8, RawKey> { (1..=103).filter_map(|k| Some((k, factory(k)?))).collect() };
        let mut s = Snapshot { normal: layer(), hypershift: layer(), look: None };
        s.normal.insert(31, RawKey { thr_low: 200, thr_high: 0, fn_id: 0x11, fn_data: vec![0x21] });
        s
    }

    #[test]
    fn layer_keeps_only_changes() {
        let p = P { data: full(), slots: BTreeMap::from([(1, full())]) };
        let v = serde_json::to_value(&p).unwrap();
        let normal = v["data"]["normal"].as_object().unwrap();
        assert_eq!(normal.len(), 2, "{normal:?}");
        assert!(normal["factory"].as_str().unwrap().contains("32-"));
        assert_eq!(normal["31"]["fn_id"], 0x11);
        assert_eq!(v["slots"]["1"]["normal"].as_object().unwrap().len(), 2);
        assert_eq!(serde_json::from_value::<P>(v).unwrap(), p);
    }

    #[test]
    fn layer_keeps_keys_without_factory() {
        let bare = (0..=255).find(|&k| factory(k).is_none()).unwrap();
        let mut s = full();
        s.hypershift.insert(bare, RawKey { thr_low: 0, thr_high: 0, fn_id: 0x02, fn_data: vec![0, 0] });
        let p = P { data: s, slots: BTreeMap::new() };
        let v = serde_json::to_value(&p).unwrap();
        assert!(v["data"]["hypershift"].get(bare.to_string()).is_some());
        assert_eq!(serde_json::from_value::<P>(v).unwrap(), p);
    }

    #[test]
    fn empty_layer_stays_empty() {
        let p = P { data: Snapshot::default(), slots: BTreeMap::new() };
        let v = serde_json::to_value(&p).unwrap();
        assert!(v["data"]["normal"].as_object().unwrap().is_empty());
        assert_eq!(serde_json::from_value::<P>(v).unwrap(), p);
    }

    #[test]
    fn reads_full_layers() {
        #[derive(Serialize)]
        struct Old {
            data: Snapshot,
            slots: BTreeMap<u8, Snapshot>,
        }
        let old = serde_json::to_string(&Old { data: full(), slots: BTreeMap::from([(2, full())]) }).unwrap();
        let p: P = serde_json::from_str(&old).unwrap();
        assert_eq!((p.data, p.slots[&2].clone()), (full(), full()));
    }

    #[test]
    fn refuses_factory_of_a_key_without_one() {
        let bare = (0..=255).find(|&k| factory(k).is_none()).unwrap();
        let text = format!(r#"{{"data": {{"normal": {{"factory": "{bare}"}}, "hypershift": {{}}}}, "slots": {{}}}}"#);
        assert!(serde_json::from_str::<P>(&text).is_err());
    }
    #[test]
    fn prints_short_containers_inline() {
        let v = serde_json::json!({"a": [0, 4], "b": {"x": 1, "y": "z"}, "long": (0..60).collect::<Vec<u32>>()});
        let text = to_text(&v).unwrap();
        assert!(text.contains(r#""a": [0, 4]"#), "{text}");
        assert!(text.contains(r#""b": {"x": 1, "y": "z"}"#), "{text}");
        assert!(text.lines().all(|l| l.len() <= 110), "{text}");
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), v);
    }

    #[test]
    fn keeps_numeric_order() {
        let m: BTreeMap<u8, u8> = [(2, 0), (10, 0)].into();
        let text = to_text(&m).unwrap();
        assert!(text.find("\"2\"").unwrap() < text.find("\"10\"").unwrap(), "{text}");
    }

    #[test]
    fn escapes_strings() {
        let v = serde_json::json!({"name": "a \"b\"\n\\"});
        assert_eq!(serde_json::from_str::<Value>(&to_text(&v).unwrap()).unwrap(), v);
    }
}
