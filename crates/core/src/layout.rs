//! Our own ANSI full-size geometry of the Huntsman V2 Analog, in key units.

use crate::keymap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutKey {
    pub key: u8,
    pub label: &'static str,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub editable: bool,
    pub round: bool,
}

/// Lit areas that are not keys; ids above every `fwID`.
pub const MEDIA: u8 = 200;
pub const DIAL: u8 = 201;
pub const EDGE: u8 = 202;
pub const WRIST: u8 = 203;

/// Custom frame column of each key, per row, in the order the keys appear in `KEYS`.
/// Taken on the device (RECON «Раскладка custom frame»); rows follow `y`.
const LED_COLS: [&[u8]; 6] = [
    &[1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
    &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21],
    &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21],
    &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 18, 19, 20],
    &[1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 16, 18, 19, 20, 21],
    &[1, 2, 3, 7, 11, 12, 13, 14, 15, 16, 17, 19, 20],
];
const FRAME_COLS: u8 = 23;

/// The Fn key sits where `RIGHT_GUI` is on a standard board; it switches layers and is not tuned.
const FN_KEY: &str = "RIGHT_GUI";

/// (keymap name, keycap label, x, y, width, height)
const KEYS: &[(&str, &str, f32, f32, f32, f32)] = &[
    ("ESC", "Esc", 0.0, 0.0, 1.0, 1.0),
    ("F1", "F1", 2.0, 0.0, 1.0, 1.0),
    ("F2", "F2", 3.0, 0.0, 1.0, 1.0),
    ("F3", "F3", 4.0, 0.0, 1.0, 1.0),
    ("F4", "F4", 5.0, 0.0, 1.0, 1.0),
    ("F5", "F5", 6.5, 0.0, 1.0, 1.0),
    ("F6", "F6", 7.5, 0.0, 1.0, 1.0),
    ("F7", "F7", 8.5, 0.0, 1.0, 1.0),
    ("F8", "F8", 9.5, 0.0, 1.0, 1.0),
    ("F9", "F9", 11.0, 0.0, 1.0, 1.0),
    ("F10", "F10", 12.0, 0.0, 1.0, 1.0),
    ("F11", "F11", 13.0, 0.0, 1.0, 1.0),
    ("F12", "F12", 14.0, 0.0, 1.0, 1.0),
    ("PRINT_SCREEN", "PrtSc", 15.25, 0.0, 1.0, 1.0),
    ("SCROLL_LOCK", "ScrLk", 16.25, 0.0, 1.0, 1.0),
    ("PAUSE", "Pause", 17.25, 0.0, 1.0, 1.0),
    ("TILDE", "`", 0.0, 1.25, 1.0, 1.0),
    ("1", "1", 1.0, 1.25, 1.0, 1.0),
    ("2", "2", 2.0, 1.25, 1.0, 1.0),
    ("3", "3", 3.0, 1.25, 1.0, 1.0),
    ("4", "4", 4.0, 1.25, 1.0, 1.0),
    ("5", "5", 5.0, 1.25, 1.0, 1.0),
    ("6", "6", 6.0, 1.25, 1.0, 1.0),
    ("7", "7", 7.0, 1.25, 1.0, 1.0),
    ("8", "8", 8.0, 1.25, 1.0, 1.0),
    ("9", "9", 9.0, 1.25, 1.0, 1.0),
    ("0", "0", 10.0, 1.25, 1.0, 1.0),
    ("HYPEN", "-", 11.0, 1.25, 1.0, 1.0),
    ("EQUAL", "=", 12.0, 1.25, 1.0, 1.0),
    ("BACKSPACE", "Backspace", 13.0, 1.25, 2.0, 1.0),
    ("INSERT", "Ins", 15.25, 1.25, 1.0, 1.0),
    ("HOME", "Home", 16.25, 1.25, 1.0, 1.0),
    ("PAGE_UP", "PgUp", 17.25, 1.25, 1.0, 1.0),
    ("NUMPAD_NUM_LOCK", "Num", 18.5, 1.25, 1.0, 1.0),
    ("NUMPAD_SLASH", "/", 19.5, 1.25, 1.0, 1.0),
    ("NUMPAD_ASTERISK", "*", 20.5, 1.25, 1.0, 1.0),
    ("NUMPAD_DASH", "-", 21.5, 1.25, 1.0, 1.0),
    ("TAB", "Tab", 0.0, 2.25, 1.5, 1.0),
    ("Q", "Q", 1.5, 2.25, 1.0, 1.0),
    ("W", "W", 2.5, 2.25, 1.0, 1.0),
    ("E", "E", 3.5, 2.25, 1.0, 1.0),
    ("R", "R", 4.5, 2.25, 1.0, 1.0),
    ("T", "T", 5.5, 2.25, 1.0, 1.0),
    ("Y", "Y", 6.5, 2.25, 1.0, 1.0),
    ("U", "U", 7.5, 2.25, 1.0, 1.0),
    ("I", "I", 8.5, 2.25, 1.0, 1.0),
    ("O", "O", 9.5, 2.25, 1.0, 1.0),
    ("P", "P", 10.5, 2.25, 1.0, 1.0),
    ("OPEN_SQUARE_BRACKET", "[", 11.5, 2.25, 1.0, 1.0),
    ("CLOSE_SQUARE_BRACKET", "]", 12.5, 2.25, 1.0, 1.0),
    ("BACKSLASH", "\\", 13.5, 2.25, 1.5, 1.0),
    ("DELETE", "Del", 15.25, 2.25, 1.0, 1.0),
    ("END", "End", 16.25, 2.25, 1.0, 1.0),
    ("PAGE_DOWN", "PgDn", 17.25, 2.25, 1.0, 1.0),
    ("NUMPAD_7", "7", 18.5, 2.25, 1.0, 1.0),
    ("NUMPAD_8", "8", 19.5, 2.25, 1.0, 1.0),
    ("NUMPAD_9", "9", 20.5, 2.25, 1.0, 1.0),
    ("NUMPAD_PLUS", "+", 21.5, 2.25, 1.0, 2.0),
    ("CAPS_LOCK", "Caps", 0.0, 3.25, 1.75, 1.0),
    ("A", "A", 1.75, 3.25, 1.0, 1.0),
    ("S", "S", 2.75, 3.25, 1.0, 1.0),
    ("D", "D", 3.75, 3.25, 1.0, 1.0),
    ("F", "F", 4.75, 3.25, 1.0, 1.0),
    ("G", "G", 5.75, 3.25, 1.0, 1.0),
    ("H", "H", 6.75, 3.25, 1.0, 1.0),
    ("J", "J", 7.75, 3.25, 1.0, 1.0),
    ("K", "K", 8.75, 3.25, 1.0, 1.0),
    ("L", "L", 9.75, 3.25, 1.0, 1.0),
    ("SEMICOLON", ";", 10.75, 3.25, 1.0, 1.0),
    ("APOSTROPHE", "'", 11.75, 3.25, 1.0, 1.0),
    ("ENTER", "Enter", 12.75, 3.25, 2.25, 1.0),
    ("NUMPAD_4", "4", 18.5, 3.25, 1.0, 1.0),
    ("NUMPAD_5", "5", 19.5, 3.25, 1.0, 1.0),
    ("NUMPAD_6", "6", 20.5, 3.25, 1.0, 1.0),
    ("LEFT_SHIFT", "Shift", 0.0, 4.25, 2.25, 1.0),
    ("Z", "Z", 2.25, 4.25, 1.0, 1.0),
    ("X", "X", 3.25, 4.25, 1.0, 1.0),
    ("C", "C", 4.25, 4.25, 1.0, 1.0),
    ("V", "V", 5.25, 4.25, 1.0, 1.0),
    ("B", "B", 6.25, 4.25, 1.0, 1.0),
    ("N", "N", 7.25, 4.25, 1.0, 1.0),
    ("M", "M", 8.25, 4.25, 1.0, 1.0),
    ("COMMA", ",", 9.25, 4.25, 1.0, 1.0),
    ("PERIOD", ".", 10.25, 4.25, 1.0, 1.0),
    ("SLASH", "/", 11.25, 4.25, 1.0, 1.0),
    ("RIGHT_SHIFT", "Shift", 12.25, 4.25, 2.75, 1.0),
    ("UP_ARROW", "↑", 16.25, 4.25, 1.0, 1.0),
    ("NUMPAD_1", "1", 18.5, 4.25, 1.0, 1.0),
    ("NUMPAD_2", "2", 19.5, 4.25, 1.0, 1.0),
    ("NUMPAD_3", "3", 20.5, 4.25, 1.0, 1.0),
    ("NUMPAD_ENTER", "Ent", 21.5, 4.25, 1.0, 2.0),
    ("LEFT_CTRL", "Ctrl", 0.0, 5.25, 1.25, 1.0),
    ("LEFT_GUI", "Win", 1.25, 5.25, 1.25, 1.0),
    ("LEFT_ALT", "Alt", 2.5, 5.25, 1.25, 1.0),
    ("SPACEBAR", "", 3.75, 5.25, 6.25, 1.0),
    ("RIGHT_ALT", "Alt", 10.0, 5.25, 1.25, 1.0),
    (FN_KEY, "Fn", 11.25, 5.25, 1.25, 1.0),
    ("APPLICATION", "Menu", 12.5, 5.25, 1.25, 1.0),
    ("RIGHT_CTRL", "Ctrl", 13.75, 5.25, 1.25, 1.0),
    ("LEFT_ARROW", "←", 15.25, 5.25, 1.0, 1.0),
    ("DOWN_ARROW", "↓", 16.25, 5.25, 1.0, 1.0),
    ("RIGHT_ARROW", "→", 17.25, 5.25, 1.0, 1.0),
    ("NUMPAD_0", "0", 18.5, 5.25, 2.0, 1.0),
    ("NUMPAD_PERIOD", ".", 20.5, 5.25, 1.0, 1.0),
];

pub fn keys() -> Vec<LayoutKey> {
    KEYS.iter()
        .map(|&(name, label, x, y, w, h)| LayoutKey {
            key: keymap::by_name(name).unwrap_or_else(|| panic!("layout names unknown key {name}")),
            label,
            x,
            y,
            w,
            h,
            editable: name != FN_KEY,
            round: false,
        })
        .collect()
}

/// Cells of the custom frame (row, column) that a key or zone lights.
pub fn cells(id: u8) -> Vec<(u8, u8)> {
    match id {
        MEDIA => vec![(0, 18)],
        DIAL => vec![(0, 21)],
        EDGE => (0..FRAME_COLS).map(|c| (6, c)).collect(),
        WRIST => (0..FRAME_COLS).map(|c| (7, c)).collect(),
        _ => {
            let Some(i) = KEYS.iter().position(|k| keymap::by_name(k.0) == Some(id)) else { return Vec::new() };
            let row = KEYS[i].3 as usize;
            let nth = KEYS[..i].iter().filter(|k| k.3 as usize == row).count();
            vec![(row as u8, LED_COLS[row][nth])]
        }
    }
}

/// Keys and lit zones for painting: the edge rings the keys, the media buttons and dial sit
/// above the numpad, the wrist rest below.
pub fn lighting_keys() -> Vec<LayoutKey> {
    let zone = |key, label, x, y, w, h, round| LayoutKey { key, label, x, y, w, h, editable: true, round };
    let mut out: Vec<LayoutKey> =
        keys().into_iter().map(|k| LayoutKey { x: k.x + 0.5, y: k.y + 0.5, editable: true, ..k }).collect();
    out.extend([
        zone(MEDIA, "● ● ●", 19.0, 0.5, 3.0, 1.0, true),
        zone(DIAL, "◎", 22.0, 0.5, 1.0, 1.0, true),
        zone(EDGE, "", 0.1, 0.1, 23.3, 0.3, false),
        zone(EDGE, "", 0.1, 6.85, 23.3, 0.3, false),
        zone(EDGE, "", 0.1, 0.1, 0.3, 7.05, false),
        zone(EDGE, "", 23.1, 0.1, 0.3, 7.05, false),
        zone(WRIST, "Подставка", 2.0, 7.5, 19.5, 1.0, false),
    ]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap;

    #[test]
    fn has_104_keys_with_unique_known_ids() {
        let keys = keys();
        assert_eq!(keys.len(), 104);
        let mut ids: Vec<u8> = keys.iter().map(|k| k.key).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 104);
        assert!(keys.iter().all(|k| keymap::name(k.key).is_some()));
    }

    #[test]
    fn keys_do_not_overlap() {
        let keys = keys();
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                let apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{} overlaps {}", a.label, b.label);
            }
        }
    }

    #[test]
    fn only_fn_is_not_editable() {
        let fixed: Vec<u8> = keys().iter().filter(|k| !k.editable).map(|k| k.key).collect();
        assert_eq!(fixed, [keymap::by_name("RIGHT_GUI").unwrap()]);
    }

    fn cell(name: &str) -> Vec<(u8, u8)> {
        cells(keymap::by_name(name).unwrap())
    }

    #[test]
    fn every_key_lights_its_own_cell() {
        let mut seen: Vec<(u8, u8)> = keys().iter().flat_map(|k| cells(k.key)).collect();
        assert_eq!(seen.len(), 104);
        assert!(seen.iter().all(|&(r, c)| r < 6 && c < 23), "{seen:?}");
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 104);
    }

    #[test]
    fn cells_match_the_hardware_table() {
        for (name, rc) in [
            ("ESC", (0, 1)),
            ("F1", (0, 3)),
            ("PAUSE", (0, 17)),
            ("TILDE", (1, 1)),
            ("NUMPAD_DASH", (1, 21)),
            ("BACKSLASH", (2, 14)),
            ("NUMPAD_PLUS", (2, 21)),
            ("ENTER", (3, 14)),
            ("NUMPAD_6", (3, 20)),
            ("Z", (4, 3)),
            ("UP_ARROW", (4, 16)),
            ("NUMPAD_ENTER", (4, 21)),
            ("SPACEBAR", (5, 7)),
            ("RIGHT_GUI", (5, 12)),
            ("NUMPAD_0", (5, 19)),
            ("NUMPAD_PERIOD", (5, 20)),
        ] {
            assert_eq!(cell(name), [rc], "{name}");
        }
    }

    #[test]
    fn zones_light_their_cells() {
        assert_eq!(cells(MEDIA), [(0, 18)]);
        assert_eq!(cells(DIAL), [(0, 21)]);
        assert_eq!(cells(EDGE), (0..23).map(|c| (6, c)).collect::<Vec<_>>());
        assert_eq!(cells(WRIST), (0..23).map(|c| (7, c)).collect::<Vec<_>>());
        assert!(cells(0).is_empty());
    }

    #[test]
    fn lighting_map_has_every_key_and_zone_selectable() {
        let map = lighting_keys();
        assert!(map.iter().all(|k| k.editable && k.x >= 0.0 && k.y >= 0.0));
        for id in keys().iter().map(|k| k.key).chain([MEDIA, DIAL, EDGE, WRIST]) {
            assert!(map.iter().any(|k| k.key == id), "{id}");
        }
    }
}
