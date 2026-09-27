//! Built-in firmware lighting effects, encoded from the device's protocol templates.

use serde::{Deserialize, Serialize};

use crate::devices::{DeviceSpec, EffectTemplate};
use crate::transport::{Error, Transport, exchange};

pub type Rgb = [u8; 3];

/// One firmware effect; which fields matter is decided by its template.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb1: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb2: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Look {
    pub effect: Effect,
    pub brightness: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Store {
    /// Shown at once, gone after a replug.
    Temporary,
    /// Survives replugs; flash with limited write endurance.
    Saved,
}

/// What the UI needs to build the controls of one effect.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EffectInfo {
    pub name: String,
    pub colors: usize,
    pub dirs: Vec<String>,
    pub speed: Option<[u8; 2]>,
    pub fast_low: bool,
}

pub fn effect_infos(d: &DeviceSpec) -> Vec<EffectInfo> {
    let p = d.protocol();
    d.lighting
        .effects
        .iter()
        .filter_map(|name| {
            let t = p.effects.get(name)?;
            Some(EffectInfo {
                name: name.clone(),
                colors: t.bytes.matches("{rgb").count(),
                dirs: t.dir.keys().cloned().collect(),
                speed: t.speed,
                fast_low: t.fast.as_deref() == Some("low"),
            })
        })
        .collect()
}

fn template<'a>(d: &'a DeviceSpec, name: &str) -> Result<&'a EffectTemplate, Error> {
    if !d.lighting.effects.iter().any(|e| e == name) {
        return Err(Error::BadArgument(format!("{}: эффект {name} не поддерживается", d.name)));
    }
    d.protocol().effects.get(name).ok_or_else(|| Error::BadArgument(format!("эффект {name} не описан в протоколе")))
}

fn missing(name: &str, field: &str) -> Error {
    Error::BadArgument(format!("эффект {name}: не задан {field}"))
}

/// Effect bytes that follow `store led`.
pub fn encode(d: &DeviceSpec, e: &Effect) -> Result<Vec<u8>, Error> {
    let t = template(d, &e.name)?;
    let mut out = Vec::new();
    for tok in t.bytes.split_whitespace() {
        match tok {
            "{rgb1}" => out.extend(e.rgb1.ok_or_else(|| missing(&e.name, "rgb1"))?),
            "{rgb2}" => out.extend(e.rgb2.ok_or_else(|| missing(&e.name, "rgb2"))?),
            "{dir}" => {
                let dir = e.dir.as_deref().ok_or_else(|| missing(&e.name, "dir"))?;
                let b = t.dir.get(dir).ok_or_else(|| Error::BadArgument(format!("направление {dir} неизвестно")))?;
                out.push(*b);
            }
            "{speed}" => {
                let s = e.speed.ok_or_else(|| missing(&e.name, "speed"))?;
                let [lo, hi] = t.speed.ok_or_else(|| missing(&e.name, "speed range"))?;
                if !(lo..=hi).contains(&s) {
                    return Err(Error::BadArgument(format!("скорость {s} вне {lo}..={hi}")));
                }
                out.push(s);
            }
            hex => out.push(u8::from_str_radix(hex, 16).expect("templates are checked by devices tests")),
        }
    }
    Ok(out)
}

pub fn decode(d: &DeviceSpec, bytes: &[u8]) -> Option<Effect> {
    let p = d.protocol();
    d.lighting.effects.iter().find_map(|name| decode_as(p.effects.get(name)?, name, bytes))
}

fn take<'a>(rest: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    let (head, tail) = rest.split_at_checked(n)?;
    *rest = tail;
    Some(head)
}

fn decode_as(t: &EffectTemplate, name: &str, bytes: &[u8]) -> Option<Effect> {
    let mut e = Effect { name: name.into(), ..Default::default() };
    let mut rest = bytes;
    for tok in t.bytes.split_whitespace() {
        match tok {
            "{rgb1}" => e.rgb1 = Some(take(&mut rest, 3)?.try_into().ok()?),
            "{rgb2}" => e.rgb2 = Some(take(&mut rest, 3)?.try_into().ok()?),
            "{dir}" => {
                let b = take(&mut rest, 1)?[0];
                e.dir = Some(t.dir.iter().find(|(_, v)| **v == b)?.0.clone());
            }
            "{speed}" => e.speed = Some(take(&mut rest, 1)?[0]),
            hex => {
                if take(&mut rest, 1)?[0] != u8::from_str_radix(hex, 16).ok()? {
                    return None;
                }
            }
        }
    }
    // The firmware answers with more bytes than the effect uses and zero-fills the rest.
    rest.iter().all(|&b| b == 0).then_some(e)
}

fn store(d: &DeviceSpec, s: Store) -> u8 {
    let st = &d.protocol().stores;
    match s {
        Store::Temporary => st.temporary,
        Store::Saved => st.saved,
    }
}

fn get_size(d: &DeviceSpec) -> u8 {
    let len = |t: &EffectTemplate| t.bytes.split_whitespace().map(|tok| if tok.starts_with("{rgb") { 3 } else { 1 }).sum::<u8>();
    2 + d.protocol().effects.values().map(len).max().unwrap_or(0)
}

pub fn set_look(t: &impl Transport, d: &DeviceSpec, s: Store, look: &Look) -> Result<(), Error> {
    let p = d.protocol();
    let args = [vec![store(d, s), d.lighting.led], encode(d, &look.effect)?].concat();
    exchange(t, p.set_effect, args.len() as u8, &args)?;
    exchange(t, p.set_brightness, 3, &[store(d, s), d.lighting.led, look.brightness]).map(|_| ())
}

/// `Ok(None)` when the keyboard holds an effect the description does not know.
pub fn get_look(t: &impl Transport, d: &DeviceSpec, s: Store) -> Result<Option<Look>, Error> {
    let p = d.protocol();
    let r = exchange(t, p.get_effect, get_size(d), &[store(d, s), d.lighting.led])?;
    let Some(effect) = decode(d, r.data().get(2..).unwrap_or_default()) else {
        return Ok(None);
    };
    let b = exchange(t, p.get_brightness, 3, &[store(d, s), d.lighting.led])?;
    let brightness = *b.data().get(2).ok_or(Error::ShortReply(p.get_brightness))?;
    Ok(Some(Look { effect, brightness }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices;
    use crate::fake::FakeKeyboard;

    const RED: Rgb = [0xFF, 0, 0];
    const BLUE: Rgb = [0, 0, 0xFF];

    fn spec() -> &'static DeviceSpec {
        devices::by_pid(0x0266).unwrap()
    }

    fn fx(name: &str) -> Effect {
        Effect { name: name.into(), ..Default::default() }
    }

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    /// Byte sequences accepted by the keyboard on 2026-09-27 (RECON «Подсветка»).
    fn cases() -> Vec<(Effect, Vec<u8>)> {
        vec![
            (fx("off"), hex("00 00 00 00")),
            (Effect { rgb1: Some(RED), ..fx("static") }, hex("01 00 00 01 FF 00 00")),
            (Effect { rgb1: Some(RED), ..fx("breathing_one") }, hex("02 01 00 01 FF 00 00")),
            (Effect { rgb1: Some(RED), rgb2: Some(BLUE), ..fx("breathing_two") }, hex("02 02 00 02 FF 00 00 00 00 FF")),
            (fx("breathing_random"), hex("02 00 00 00")),
            (fx("spectrum"), hex("03 00 00 00")),
            (Effect { dir: Some("right".into()), speed: Some(0x28), ..fx("wave") }, hex("04 02 28 00")),
            (Effect { speed: Some(2), rgb1: Some(RED), ..fx("reactive") }, hex("05 00 02 01 FF 00 00")),
            (Effect { speed: Some(2), ..fx("starlight_random") }, hex("07 00 02 00")),
            (Effect { speed: Some(2), rgb1: Some(RED), ..fx("starlight_one") }, hex("07 00 02 01 FF 00 00")),
            (
                Effect { speed: Some(2), rgb1: Some(RED), rgb2: Some(BLUE), ..fx("starlight_two") },
                hex("07 00 02 02 FF 00 00 00 00 FF"),
            ),
        ]
    }

    #[test]
    fn encodes_every_effect_as_the_keyboard_expects() {
        for (e, bytes) in cases() {
            assert_eq!(encode(spec(), &e).unwrap(), bytes, "{}", e.name);
        }
    }

    #[test]
    fn decodes_replies_even_when_padded_with_zeros() {
        for (e, bytes) in cases() {
            assert_eq!(decode(spec(), &bytes).as_ref(), Some(&e), "{}", e.name);
            let padded = [bytes, vec![0; 6]].concat();
            assert_eq!(decode(spec(), &padded).as_ref(), Some(&e), "{} padded", e.name);
        }
    }

    #[test]
    fn unknown_effect_bytes_decode_to_none() {
        assert_eq!(decode(spec(), &hex("08 00 00")), None);
        assert_eq!(decode(spec(), &hex("01 00 00 01 FF")), None);
    }

    #[test]
    fn rejects_bad_effects_before_touching_the_device() {
        let kb = FakeKeyboard::new(&[]);
        let bad = [
            Effect { speed: Some(5), rgb1: Some(RED), ..fx("reactive") },
            Effect { speed: Some(0), ..fx("starlight_random") },
            Effect { dir: Some("up".into()), speed: Some(1), ..fx("wave") },
            fx("static"),
            fx("wheel"),
        ];
        for e in bad {
            let look = Look { effect: e.clone(), brightness: 0x80 };
            assert!(matches!(set_look(&kb, spec(), Store::Temporary, &look), Err(Error::BadArgument(_))), "{e:?}");
        }
        assert!(kb.sent.borrow().is_empty());
    }

    #[test]
    fn temporary_look_leaves_the_saved_store_alone() {
        let kb = FakeKeyboard::new(&[]);
        let look = Look { effect: Effect { rgb1: Some(RED), ..fx("static") }, brightness: 0x80 };
        set_look(&kb, spec(), Store::Temporary, &look).unwrap();
        assert_eq!(get_look(&kb, spec(), Store::Temporary).unwrap(), Some(look));
        let saved = get_look(&kb, spec(), Store::Saved).unwrap().unwrap();
        assert_eq!((saved.effect, saved.brightness), (fx("spectrum"), 0xF2));
    }

    #[test]
    fn saved_look_round_trips() {
        let kb = FakeKeyboard::new(&[]);
        let look = Look { effect: Effect { dir: Some("left".into()), speed: Some(0x10), ..fx("wave") }, brightness: 0 };
        set_look(&kb, spec(), Store::Saved, &look).unwrap();
        assert_eq!(get_look(&kb, spec(), Store::Saved).unwrap(), Some(look));
    }

    #[test]
    fn unknown_effect_in_the_keyboard_is_none() {
        let kb = FakeKeyboard::new(&[]);
        kb.effects.borrow_mut().insert(0, hex("08 00 00"));
        assert_eq!(get_look(&kb, spec(), Store::Temporary).unwrap(), None);
    }

    #[test]
    fn describes_effects_for_the_ui() {
        let infos = effect_infos(spec());
        assert_eq!(infos.len(), spec().lighting.effects.len());
        let wave = infos.iter().find(|i| i.name == "wave").unwrap();
        assert_eq!((wave.colors, wave.dirs.clone(), wave.speed, wave.fast_low), (0, vec!["left".into(), "right".into()], Some([1, 255]), true));
        let two = infos.iter().find(|i| i.name == "breathing_two").unwrap();
        assert_eq!((two.colors, two.speed), (2, None));
    }
}
