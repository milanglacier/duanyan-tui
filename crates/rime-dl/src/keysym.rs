//! X11 keysyms and modifier masks as understood by `RimeProcessKey`, plus a
//! parser for rime's textual key notation (`Control+grave`, `Release+Shift_L`).
//!
//! librime does not export its key table, so the subset needed by a terminal
//! frontend is reproduced here.

use std::fmt;

pub mod mask {
    pub const SHIFT: u32 = 1 << 0;
    pub const LOCK: u32 = 1 << 1;
    pub const CONTROL: u32 = 1 << 2;
    pub const MOD1: u32 = 1 << 3;
    pub const ALT: u32 = MOD1;
    pub const MOD2: u32 = 1 << 4;
    pub const MOD3: u32 = 1 << 5;
    pub const MOD4: u32 = 1 << 6;
    pub const MOD5: u32 = 1 << 7;
    pub const SUPER: u32 = 1 << 26;
    pub const HYPER: u32 = 1 << 27;
    pub const META: u32 = 1 << 28;
    pub const RELEASE: u32 = 1 << 30;
}

pub mod sym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const LINEFEED: u32 = 0xff0a;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const DELETE: u32 = 0xffff;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const END: u32 = 0xff57;
    pub const INSERT: u32 = 0xff63;
    pub const MENU: u32 = 0xff67;
    pub const ISO_LEFT_TAB: u32 = 0xfe20;
    pub const F1: u32 = 0xffbe;
    pub const SHIFT_L: u32 = 0xffe1;
    pub const SHIFT_R: u32 = 0xffe2;
    pub const CONTROL_L: u32 = 0xffe3;
    pub const CONTROL_R: u32 = 0xffe4;
    pub const CAPS_LOCK: u32 = 0xffe5;
    pub const SHIFT_LOCK: u32 = 0xffe6;
    pub const META_L: u32 = 0xffe7;
    pub const META_R: u32 = 0xffe8;
    pub const ALT_L: u32 = 0xffe9;
    pub const ALT_R: u32 = 0xffea;
    pub const SUPER_L: u32 = 0xffeb;
    pub const SUPER_R: u32 = 0xffec;
    pub const HYPER_L: u32 = 0xffed;
    pub const HYPER_R: u32 = 0xffee;
    pub const SPACE: u32 = 0x20;
}

/// A key event in rime's terms: a keysym plus a modifier mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RimeKey {
    pub keycode: u32,
    pub mask: u32,
}

impl RimeKey {
    pub const fn new(keycode: u32, mask: u32) -> Self {
        Self { keycode, mask }
    }

    pub fn is_release(&self) -> bool {
        self.mask & mask::RELEASE != 0
    }

    pub fn released(self) -> Self {
        Self::new(self.keycode, self.mask | mask::RELEASE)
    }

    /// Parses rime's notation, e.g. `Control+grave`, `Shift_L`,
    /// `Release+Shift_L`, `Control+Shift+a`, `a`.
    pub fn parse(s: &str) -> Result<Self, ParseKeyError> {
        let err = || ParseKeyError(s.to_owned());
        if s.is_empty() {
            return Err(err());
        }
        // Same rule as librime's KeyEvent::Parse: the last `+`-separated
        // component is the key name; a lone character is the key itself.
        let (mods, name) = if s.chars().count() == 1 {
            ("", s)
        } else {
            match s.rfind('+') {
                Some(i) => (&s[..i], &s[i + 1..]),
                None => ("", s),
            }
        };
        let mut m = 0;
        if !mods.is_empty() {
            for part in mods.split('+') {
                m |= modifier_by_name(part).ok_or_else(err)?;
            }
        }
        let keycode = keycode_by_name(name).ok_or_else(err)?;
        Ok(Self::new(keycode, m))
    }
}

impl fmt::Display for RimeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, bit) in MODIFIERS {
            if self.mask & bit != 0 {
                write!(f, "{name}+")?;
            }
        }
        match keycode_name(self.keycode) {
            Some(name) => f.write_str(name),
            None => match char::from_u32(self.keycode) {
                Some(c) if !c.is_control() => write!(f, "{c}"),
                _ => write!(f, "0x{:04x}", self.keycode),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid rime key: {0:?}")]
pub struct ParseKeyError(pub String);

const MODIFIERS: &[(&str, u32)] = &[
    ("Shift", mask::SHIFT),
    ("Lock", mask::LOCK),
    ("Control", mask::CONTROL),
    ("Alt", mask::ALT),
    ("Mod2", mask::MOD2),
    ("Mod3", mask::MOD3),
    ("Mod4", mask::MOD4),
    ("Mod5", mask::MOD5),
    ("Super", mask::SUPER),
    ("Hyper", mask::HYPER),
    ("Meta", mask::META),
    ("Release", mask::RELEASE),
];

pub fn modifier_by_name(name: &str) -> Option<u32> {
    MODIFIERS.iter().find(|(n, _)| *n == name).map(|(_, m)| *m)
}

const NAMED_KEYS: &[(&str, u32)] = &[
    ("space", 0x20),
    ("exclam", 0x21),
    ("quotedbl", 0x22),
    ("numbersign", 0x23),
    ("dollar", 0x24),
    ("percent", 0x25),
    ("ampersand", 0x26),
    ("apostrophe", 0x27),
    ("parenleft", 0x28),
    ("parenright", 0x29),
    ("asterisk", 0x2a),
    ("plus", 0x2b),
    ("comma", 0x2c),
    ("minus", 0x2d),
    ("period", 0x2e),
    ("slash", 0x2f),
    ("colon", 0x3a),
    ("semicolon", 0x3b),
    ("less", 0x3c),
    ("equal", 0x3d),
    ("greater", 0x3e),
    ("question", 0x3f),
    ("at", 0x40),
    ("bracketleft", 0x5b),
    ("backslash", 0x5c),
    ("bracketright", 0x5d),
    ("asciicircum", 0x5e),
    ("underscore", 0x5f),
    ("grave", 0x60),
    ("braceleft", 0x7b),
    ("bar", 0x7c),
    ("braceright", 0x7d),
    ("asciitilde", 0x7e),
    ("BackSpace", sym::BACKSPACE),
    ("Tab", sym::TAB),
    ("Linefeed", sym::LINEFEED),
    ("Clear", 0xff0b),
    ("Return", sym::RETURN),
    ("Pause", 0xff13),
    ("Scroll_Lock", 0xff14),
    ("Sys_Req", 0xff15),
    ("Escape", sym::ESCAPE),
    ("Delete", sym::DELETE),
    ("Home", sym::HOME),
    ("Left", sym::LEFT),
    ("Up", sym::UP),
    ("Right", sym::RIGHT),
    ("Down", sym::DOWN),
    ("Prior", sym::PAGE_UP),
    ("Page_Up", sym::PAGE_UP),
    ("Next", sym::PAGE_DOWN),
    ("Page_Down", sym::PAGE_DOWN),
    ("End", sym::END),
    ("Begin", 0xff58),
    ("Select", 0xff60),
    ("Print", 0xff61),
    ("Execute", 0xff62),
    ("Insert", sym::INSERT),
    ("Undo", 0xff65),
    ("Redo", 0xff66),
    ("Menu", sym::MENU),
    ("Find", 0xff68),
    ("Cancel", 0xff69),
    ("Help", 0xff6a),
    ("Break", 0xff6b),
    ("Num_Lock", 0xff7f),
    ("ISO_Left_Tab", sym::ISO_LEFT_TAB),
    ("Shift_L", sym::SHIFT_L),
    ("Shift_R", sym::SHIFT_R),
    ("Control_L", sym::CONTROL_L),
    ("Control_R", sym::CONTROL_R),
    ("Caps_Lock", sym::CAPS_LOCK),
    ("Shift_Lock", sym::SHIFT_LOCK),
    ("Meta_L", sym::META_L),
    ("Meta_R", sym::META_R),
    ("Alt_L", sym::ALT_L),
    ("Alt_R", sym::ALT_R),
    ("Super_L", sym::SUPER_L),
    ("Super_R", sym::SUPER_R),
    ("Hyper_L", sym::HYPER_L),
    ("Hyper_R", sym::HYPER_R),
];

/// Resolves a rime key name. Besides the named keys, single printable ASCII
/// characters (`a`, `Z`, `0`) and `F1`..`F35` are accepted.
pub fn keycode_by_name(name: &str) -> Option<u32> {
    if let Some((_, code)) = NAMED_KEYS.iter().find(|(n, _)| *n == name) {
        return Some(*code);
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_ascii_graphic() {
            return Some(c as u32);
        }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=35).contains(&n) {
            return Some(sym::F1 + n - 1);
        }
    None
}

/// The canonical name of a keysym, if it has one other than its character.
pub fn keycode_name(code: u32) -> Option<&'static str> {
    if (sym::F1..sym::F1 + 35).contains(&code) {
        const F: [&str; 35] = [
            "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13",
            "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24", "F25",
            "F26", "F27", "F28", "F29", "F30", "F31", "F32", "F33", "F34", "F35",
        ];
        return Some(F[(code - sym::F1) as usize]);
    }
    NAMED_KEYS.iter().find(|(_, c)| *c == code).map(|(n, _)| *n)
}

/// Keysym for a Unicode character: Latin-1 maps to itself, everything else
/// uses the `0x01000000 + codepoint` Unicode keysym range.
pub fn keycode_for_char(c: char) -> u32 {
    let cp = c as u32;
    if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) {
        cp
    } else {
        0x0100_0000 + cp
    }
}

/// Whether the keysym is a bare modifier key (Shift_L, Caps_Lock, ...).
pub fn is_modifier_keycode(code: u32) -> bool {
    (sym::SHIFT_L..=sym::HYPER_R).contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_and_modified() {
        assert_eq!(RimeKey::parse("a").unwrap(), RimeKey::new(0x61, 0));
        assert_eq!(
            RimeKey::parse("Shift_L").unwrap(),
            RimeKey::new(sym::SHIFT_L, 0)
        );
        assert_eq!(
            RimeKey::parse("Control+grave").unwrap(),
            RimeKey::new(0x60, mask::CONTROL)
        );
        assert_eq!(
            RimeKey::parse("Release+Shift_R").unwrap(),
            RimeKey::new(sym::SHIFT_R, mask::RELEASE)
        );
        assert_eq!(
            RimeKey::parse("Control+Shift+F4").unwrap(),
            RimeKey::new(sym::F1 + 3, mask::CONTROL | mask::SHIFT)
        );
        assert_eq!(RimeKey::parse("+").unwrap(), RimeKey::new(0x2b, 0));
        assert_eq!(
            RimeKey::parse("Control+plus").unwrap(),
            RimeKey::new(0x2b, mask::CONTROL)
        );
        assert_eq!(
            RimeKey::parse("Alt+comma").unwrap(),
            RimeKey::new(0x2c, mask::ALT)
        );
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(RimeKey::parse("").is_err());
        assert!(RimeKey::parse("Ctrl+a").is_err());
        assert!(RimeKey::parse("Control+").is_err());
        assert!(RimeKey::parse("Mod1+a").is_err());
        assert!(RimeKey::parse("Shift_X").is_err());
        assert!(RimeKey::parse("F36").is_err());
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "Control+grave",
            "Shift_L",
            "Release+Shift_L",
            "Shift+Control+F4",
            "a",
        ] {
            assert_eq!(RimeKey::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn unicode_keysyms() {
        assert_eq!(keycode_for_char('a'), 0x61);
        assert_eq!(keycode_for_char('é'), 0xe9);
        assert_eq!(keycode_for_char('中'), 0x0100_4e2d);
    }
}
