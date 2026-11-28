//! Key names to firmware key ids (`fwID`), from Synapse `InputIDToMapping` for 1532:0266.

pub const KEYS: &[(&str, u8)] = &[
    ("TILDE", 1),
    ("1", 2),
    ("2", 3),
    ("3", 4),
    ("4", 5),
    ("5", 6),
    ("6", 7),
    ("7", 8),
    ("8", 9),
    ("9", 10),
    ("0", 11),
    ("HYPEN", 12),
    ("EQUAL", 13),
    ("YEN", 14),
    ("BACKSPACE", 15),
    ("TAB", 16),
    ("Q", 17),
    ("W", 18),
    ("E", 19),
    ("R", 20),
    ("T", 21),
    ("Y", 22),
    ("U", 23),
    ("I", 24),
    ("O", 25),
    ("P", 26),
    ("OPEN_SQUARE_BRACKET", 27),
    ("CLOSE_SQUARE_BRACKET", 28),
    ("BACKSLASH", 29),
    ("CAPS_LOCK", 30),
    ("A", 31),
    ("S", 32),
    ("D", 33),
    ("F", 34),
    ("G", 35),
    ("H", 36),
    ("J", 37),
    ("K", 38),
    ("L", 39),
    ("SEMICOLON", 40),
    ("APOSTROPHE", 41),
    ("NON_US_POUND", 42),
    ("ENTER", 43),
    ("LEFT_SHIFT", 44),
    ("NON_US_BACKSLASH", 45),
    ("Z", 46),
    ("X", 47),
    ("C", 48),
    ("V", 49),
    ("B", 50),
    ("N", 51),
    ("M", 52),
    ("COMMA", 53),
    ("PERIOD", 54),
    ("SLASH", 55),
    ("RO", 56),
    ("RIGHT_SHIFT", 57),
    ("LEFT_CTRL", 58),
    ("RIGHT_GUI", 59),
    ("LEFT_ALT", 60),
    ("SPACEBAR", 61),
    ("RIGHT_ALT", 62),
    ("RIGHT_CTRL", 64),
    ("INSERT", 75),
    ("DELETE", 76),
    ("LEFT_ARROW", 79),
    ("HOME", 80),
    ("END", 81),
    ("UP_ARROW", 83),
    ("DOWN_ARROW", 84),
    ("PAGE_UP", 85),
    ("PAGE_DOWN", 86),
    ("RIGHT_ARROW", 89),
    ("NUMPAD_NUM_LOCK", 90),
    ("NUMPAD_7", 91),
    ("NUMPAD_4", 92),
    ("NUMPAD_1", 93),
    ("NUMPAD_SLASH", 95),
    ("NUMPAD_8", 96),
    ("NUMPAD_5", 97),
    ("NUMPAD_2", 98),
    ("NUMPAD_0", 99),
    ("NUMPAD_ASTERISK", 100),
    ("NUMPAD_9", 101),
    ("NUMPAD_6", 102),
    ("NUMPAD_3", 103),
    ("NUMPAD_PERIOD", 104),
    ("NUMPAD_DASH", 105),
    ("NUMPAD_PLUS", 106),
    ("NUMPAD_EQUAL", 107),
    ("NUMPAD_ENTER", 108),
    ("ESC", 110),
    ("F1", 112),
    ("F2", 113),
    ("F3", 114),
    ("F4", 115),
    ("F5", 116),
    ("F6", 117),
    ("F7", 118),
    ("F8", 119),
    ("F9", 120),
    ("F10", 121),
    ("F11", 122),
    ("F12", 123),
    ("PRINT_SCREEN", 124),
    ("SCROLL_LOCK", 125),
    ("PAUSE", 126),
    ("LEFT_GUI", 127),
    ("APPLICATION", 129),
    ("MUHENKAN", 150),
    ("KATAKANA_HIRAGANA", 151),
    ("HANJA", 152),
    ("HANGEUL", 153),
    ("HENKAN", 154),
];

pub fn by_name(name: &str) -> Option<u8> {
    KEYS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, id)| id)
}

pub fn name(id: u8) -> Option<&'static str> {
    KEYS.iter().find(|&&(_, i)| i == id).map(|&(n, _)| n)
}

/// Keyboard/Keypad page usage of a fwID (modifiers are 0xE0..=0xE7).
pub fn usage(id: u8) -> Option<u8> {
    Some(match id {
        1 => 0x35,
        2..=10 => 0x1E + (id - 2),
        11 => 0x27,
        12 => 0x2D,
        13 => 0x2E,
        14 => 0x89,
        15 => 0x2A,
        16 => 0x2B,
        17 => 0x14,
        18 => 0x1A,
        19 => 0x08,
        20 => 0x15,
        21 => 0x17,
        22 => 0x1C,
        23 => 0x18,
        24 => 0x0C,
        25 => 0x12,
        26 => 0x13,
        27 => 0x2F,
        28 => 0x30,
        29 => 0x31,
        30 => 0x39,
        31 => 0x04,
        32 => 0x16,
        33 => 0x07,
        34 => 0x09,
        35 => 0x0A,
        36 => 0x0B,
        37 => 0x0D,
        38 => 0x0E,
        39 => 0x0F,
        40 => 0x33,
        41 => 0x34,
        42 => 0x32,
        43 => 0x28,
        44 => 0xE1,
        45 => 0x64,
        46 => 0x1D,
        47 => 0x1B,
        48 => 0x06,
        49 => 0x19,
        50 => 0x05,
        51 => 0x11,
        52 => 0x10,
        53 => 0x36,
        54 => 0x37,
        55 => 0x38,
        56 => 0x87,
        57 => 0xE5,
        58 => 0xE0,
        59 => 0xE7,
        60 => 0xE2,
        61 => 0x2C,
        62 => 0xE6,
        64 => 0xE4,
        75 => 0x49,
        76 => 0x4C,
        79 => 0x50,
        80 => 0x4A,
        81 => 0x4D,
        83 => 0x52,
        84 => 0x51,
        85 => 0x4B,
        86 => 0x4E,
        89 => 0x4F,
        90 => 0x53,
        91 => 0x5F,
        92 => 0x5C,
        93 => 0x59,
        95 => 0x54,
        96 => 0x60,
        97 => 0x5D,
        98 => 0x5A,
        99 => 0x62,
        100 => 0x55,
        101 => 0x61,
        102 => 0x5E,
        103 => 0x5B,
        104 => 0x63,
        105 => 0x56,
        106 => 0x57,
        107 => 0x67,
        108 => 0x58,
        110 => 0x29,
        112..=121 => 0x3A + (id - 112),
        122 => 0x44,
        123 => 0x45,
        124 => 0x46,
        125 => 0x47,
        126 => 0x48,
        127 => 0xE3,
        129 => 0x65,
        150 => 0x8B,
        151 => 0x88,
        152 => 0x91,
        153 => 0x90,
        154 => 0x8A,
        _ => return None,
    })
}

/// Consumer page usage of a media action.
pub fn media_usage(m: crate::rapid::Media) -> u16 {
    use crate::rapid::Media;
    match m {
        Media::Prev => 0xB6,
        Media::Play => 0xCD,
        Media::Next => 0xB5,
        Media::Stop => 0xB7,
        Media::Mute => 0xE2,
        Media::VolumeUp => 0xE9,
        Media::VolumeDown => 0xEA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_key_has_a_usage() {
        for &(n, id) in KEYS {
            assert!(usage(id).is_some(), "{n}");
        }
    }

    #[test]
    fn usages_are_unique() {
        let mut seen = std::collections::HashMap::new();
        for &(n, id) in KEYS {
            let u = usage(id).unwrap();
            if let Some(prev) = seen.insert(u, n) {
                panic!("{n} duplicates {prev}");
            }
        }
    }

    #[test]
    fn disputed_keys() {
        let by = |n| usage(by_name(n).unwrap());
        assert_eq!(by("PAUSE"), Some(0x48));
        assert_eq!(by("PRINT_SCREEN"), Some(0x46));
        assert_eq!(by("SCROLL_LOCK"), Some(0x47));
        assert_eq!(by("NUMPAD_NUM_LOCK"), Some(0x53));
        assert_eq!(by("NUMPAD_ENTER"), Some(0x58));
        assert_eq!(by("NUMPAD_SLASH"), Some(0x54));
        assert_eq!(by("RIGHT_CTRL"), Some(0xE4));
        assert_eq!(by("RIGHT_ALT"), Some(0xE6));
        assert_eq!(by("RIGHT_GUI"), Some(0xE7));
        assert_eq!(by("APPLICATION"), Some(0x65));
        assert_eq!(by("NON_US_BACKSLASH"), Some(0x64));
        assert_eq!(by("NON_US_POUND"), Some(0x32));
        assert_eq!(by("BACKSLASH"), Some(0x31));
        assert_eq!(by("YEN"), Some(0x89));
        assert_eq!(by("RO"), Some(0x87));
        assert_eq!(by("MUHENKAN"), Some(0x8B));
        assert_eq!(by("HENKAN"), Some(0x8A));
        assert_eq!(by("KATAKANA_HIRAGANA"), Some(0x88));
        assert_eq!(by("HANGEUL"), Some(0x90));
        assert_eq!(by("HANJA"), Some(0x91));
    }

    #[test]
    fn media_usages() {
        use crate::rapid::Media::*;
        let all = [Prev, Play, Next, Stop, Mute, VolumeUp, VolumeDown];
        assert_eq!(all.map(media_usage), [0xB6, 0xCD, 0xB5, 0xB7, 0xE2, 0xE9, 0xEA]);
    }
}
