//! Built-in firmware lighting effects, encoded from the device's protocol templates.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::devices::{DeviceSpec, EffectTemplate};
use crate::layout;
use crate::transport::{Error, Transport, exchange};

pub type Rgb = [u8; 3];

/// Per-key colors; the firmware keeps them only in the temporary store.
pub const CUSTOM: &str = "custom";
const FRAME_ROWS: u8 = 8;
const FRAME_COLS: usize = 23;

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
    /// `custom` only: key `fwID` or `layout` zone id to color; the rest stays dark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<BTreeMap<u8, Rgb>>,
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
    /// Flash profile 1-5; survives replugs, limited write endurance.
    Slot(u8),
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
        return Err(Error::BadArgument(format!("{}: effect {name} is not supported", d.name)));
    }
    d.protocol().effects.get(name).ok_or_else(|| Error::BadArgument(format!("effect {name} is not in the protocol")))
}

fn missing(name: &str, field: &str) -> Error {
    Error::BadArgument(format!("effect {name}: {field} is missing"))
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
                let b = t.dir.get(dir).ok_or_else(|| Error::BadArgument(format!("unknown direction {dir}")))?;
                out.push(*b);
            }
            "{speed}" => {
                let s = e.speed.ok_or_else(|| missing(&e.name, "speed"))?;
                let [lo, hi] = t.speed.ok_or_else(|| missing(&e.name, "speed range"))?;
                if !(lo..=hi).contains(&s) {
                    return Err(Error::BadArgument(format!("speed {s} is outside {lo}..={hi}")));
                }
                out.push(s);
            }
            hex => out.push(u8::from_str_radix(hex, 16).expect("templates are checked by devices tests")),
        }
    }
    Ok(out)
}

fn decode(d: &DeviceSpec, bytes: &[u8]) -> Option<Effect> {
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
    match s {
        Store::Temporary => d.protocol().stores.temporary,
        Store::Slot(n) => n,
    }
}

fn get_size(d: &DeviceSpec) -> u8 {
    let len = |t: &EffectTemplate| t.bytes.split_whitespace().map(|tok| if tok.starts_with("{rgb") { 3 } else { 1 }).sum::<u8>();
    2 + d.protocol().effects.values().map(len).max().unwrap_or(0)
}

fn frame(colors: &BTreeMap<u8, Rgb>) -> Vec<[Rgb; FRAME_COLS]> {
    let mut rows = vec![[[0; 3]; FRAME_COLS]; FRAME_ROWS as usize];
    for (&id, &rgb) in colors {
        for (r, c) in layout::cells(id) {
            rows[r as usize][c as usize] = rgb;
        }
    }
    rows
}

fn set_frame(t: &impl Transport, d: &DeviceSpec, look: &Look) -> Result<(), Error> {
    let set = d.protocol().set_frame.ok_or_else(|| Error::BadArgument(format!("{}: custom layout is not supported", d.name)))?;
    let colors = look.effect.colors.as_ref().ok_or_else(|| missing(CUSTOM, "colors"))?;
    for (row, cells) in frame(colors).iter().enumerate() {
        let args = [&[store(d, Store::Temporary), 0, row as u8, 0, FRAME_COLS as u8 - 1][..], cells.as_flattened()].concat();
        exchange(t, set, args.len() as u8, &args)?;
    }
    Ok(())
}

pub fn set_look(t: &impl Transport, d: &DeviceSpec, s: Store, look: &Look) -> Result<(), Error> {
    let p = d.protocol();
    if look.effect.name == CUSTOM {
        if s != Store::Temporary {
            return Err(Error::BadArgument("the custom layout cannot be saved to the keyboard".into()));
        }
        template(d, CUSTOM)?;
        set_frame(t, d, look)?;
    }
    let args = [vec![store(d, s), d.lighting.led], encode(d, &look.effect)?].concat();
    exchange(t, p.set_effect, args.len() as u8, &args)?;
    set_brightness(t, d, s, look.brightness)
}

pub fn set_brightness(t: &impl Transport, d: &DeviceSpec, s: Store, value: u8) -> Result<(), Error> {
    exchange(t, d.protocol().set_brightness, 3, &[store(d, s), d.lighting.led, value]).map(|_| ())
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

    #[test]
    fn brightness_alone_keeps_the_effect() {
        let kb = FakeKeyboard::new(&[31]);
        let look = Look { effect: fx("spectrum"), brightness: 0x80 };
        set_look(&kb, spec(), Store::Temporary, &look).unwrap();
        let sent = kb.sent.borrow().len();
        set_brightness(&kb, spec(), Store::Temporary, 0x40).unwrap();
        assert_eq!(kb.sent.borrow().len(), sent + 1);
        assert_eq!(kb.brightness.borrow()[&0], 0x40);
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
            (Effect { rgb1: Some(RED), ..fx("reactive_key_one") }, hex("05 00 02 01 FF 00 00")),
            (fx("reactive_key_random"), hex("05 00 02 00")),
            (Effect { rgb1: Some(RED), ..fx("reactive_ripple_one") }, hex("06 00 02 01 FF 00 00")),
            (fx("reactive_ripple_random"), hex("06 00 02 00")),
            (Effect { speed: Some(2), ..fx("starlight_random") }, hex("07 00 02 00")),
            (Effect { speed: Some(2), rgb1: Some(RED), ..fx("starlight_one") }, hex("07 00 02 01 FF 00 00")),
            (
                Effect { speed: Some(2), rgb1: Some(RED), rgb2: Some(BLUE), ..fx("starlight_two") },
                hex("07 00 02 02 FF 00 00 00 00 FF"),
            ),
            (fx("fire"), hex("09 00 00 00")),
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
        assert_eq!(decode(spec(), &hex("0A 00 00 00")), None);
        assert_eq!(decode(spec(), &hex("01 00 00 01 FF")), None);
    }

    #[test]
    fn rejects_bad_effects_before_touching_the_device() {
        let kb = FakeKeyboard::new(&[]);
        let bad = [
            Effect { dir: Some("left".into()), speed: Some(0), ..fx("wave") },
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
        let saved = get_look(&kb, spec(), Store::Slot(1)).unwrap().unwrap();
        assert_eq!((saved.effect, saved.brightness), (fx("spectrum"), 0xF2));
    }

    #[test]
    fn saved_look_round_trips() {
        let kb = FakeKeyboard::new(&[]);
        let look = Look { effect: Effect { dir: Some("left".into()), speed: Some(0x10), ..fx("wave") }, brightness: 0 };
        set_look(&kb, spec(), Store::Slot(1), &look).unwrap();
        assert_eq!(get_look(&kb, spec(), Store::Slot(1)).unwrap(), Some(look));
    }

    #[test]
    fn unknown_effect_in_the_keyboard_is_none() {
        let kb = FakeKeyboard::new(&[]);
        kb.effects.borrow_mut().insert(0, hex("0A 00 00 00"));
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

    fn custom(colors: &[(u8, Rgb)]) -> Look {
        Look { effect: Effect { colors: Some(colors.iter().copied().collect()), ..fx(CUSTOM) }, brightness: 0x80 }
    }

    #[test]
    fn custom_look_paints_the_frame_and_shows_it() {
        let kb = FakeKeyboard::new(&[]);
        let esc = crate::keymap::by_name("ESC").unwrap();
        let space = crate::keymap::by_name("SPACEBAR").unwrap();
        let green = [0, 0xFF, 0];
        set_look(&kb, spec(), Store::Temporary, &custom(&[(esc, RED), (space, green), (layout::EDGE, BLUE)])).unwrap();
        let frame = kb.frame.borrow();
        let at = |r: u8, c: usize| frame[&r][c * 3..c * 3 + 3].to_vec();
        assert_eq!(frame.len(), 8);
        assert!(frame.values().all(|row| row.len() == 23 * 3));
        assert_eq!(at(0, 1), RED);
        assert_eq!(at(0, 0), [0, 0, 0]);
        assert_eq!(at(5, 7), green);
        assert!((0..23).all(|c| at(6, c) == BLUE));
        assert!((0..23).all(|c| at(7, c) == [0, 0, 0]));
        assert_eq!(kb.effects.borrow()[&0], hex("08 00 00 00"));
        assert_eq!(kb.brightness.borrow()[&0], 0x80);
    }

    #[test]
    fn custom_look_cannot_go_to_the_flash() {
        let kb = FakeKeyboard::new(&[]);
        assert!(matches!(set_look(&kb, spec(), Store::Slot(1), &custom(&[])), Err(Error::BadArgument(_))));
        assert!(matches!(set_look(&kb, spec(), Store::Temporary, &Look { effect: fx(CUSTOM), brightness: 1 }), Err(Error::BadArgument(_))));
        assert!(kb.sent.borrow().is_empty());
    }

    #[test]
    fn custom_effect_is_offered_and_recognised() {
        assert!(effect_infos(spec()).iter().any(|i| i.name == CUSTOM));
        assert_eq!(decode(spec(), &hex("08 00 00 00 00")), Some(fx(CUSTOM)));
    }
}
