//! Terminal key notation (`alt+K`, `ctrl+[`, `shift+f1`), normalization of
//! crossterm events, and translation of events into rime keys.
//!
//! Printable keys are written as the character the terminal produces, so
//! shift is folded into the character: `alt+K` is Alt+Shift+k and differs
//! from `alt+k`. Named keys keep an explicit `shift+`.

use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode};
use rime_dl::RimeKey;
use rime_dl::keysym::{keycode_for_char, mask, sym};

pub const CTRL: u8 = 1 << 0;
pub const ALT: u8 = 1 << 1;
pub const SUPER: u8 = 1 << 2;
pub const SHIFT: u8 = 1 << 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Named {
    Enter,
    Tab,
    Esc,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    Space,
    F(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Char(char),
    Named(Named),
}

/// A normalized key chord. Invariant: `Key::Char` never carries `SHIFT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeySpec {
    pub mods: u8,
    pub key: Key,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyParseError {
    #[error("empty key")]
    Empty,
    #[error("unknown modifier {0:?} (expected ctrl, alt, super or shift)")]
    Modifier(String),
    #[error("unknown key name {0:?}")]
    Name(String),
    #[error(
        "{0:?}: shift+ is only allowed with letters and named keys; write the shifted character instead"
    )]
    ShiftedSymbol(String),
}

const NAMES: &[(&str, Named)] = &[
    ("enter", Named::Enter),
    ("return", Named::Enter),
    ("tab", Named::Tab),
    ("esc", Named::Esc),
    ("escape", Named::Esc),
    ("backspace", Named::Backspace),
    ("delete", Named::Delete),
    ("del", Named::Delete),
    ("insert", Named::Insert),
    ("home", Named::Home),
    ("end", Named::End),
    ("pageup", Named::PageUp),
    ("pagedown", Named::PageDown),
    ("up", Named::Up),
    ("down", Named::Down),
    ("left", Named::Left),
    ("right", Named::Right),
    ("space", Named::Space),
];

impl KeySpec {
    pub const fn new(mods: u8, key: Key) -> Self {
        Self { mods, key }
    }

    pub fn parse(s: &str) -> Result<Self, KeyParseError> {
        if s.is_empty() {
            return Err(KeyParseError::Empty);
        }
        // The last `+`-separated component is the key; `ctrl++` means plus.
        let (mods_str, key_str) = if s == "+" {
            ("", "+")
        } else if let Some(prefix) = s.strip_suffix("++") {
            (prefix, "+")
        } else {
            match s.rsplit_once('+') {
                Some((m, k)) if !k.is_empty() => (m, k),
                Some(_) => return Err(KeyParseError::Name(s.to_owned())),
                None => ("", s),
            }
        };
        let mut mods = 0;
        if !mods_str.is_empty() {
            for m in mods_str.split('+') {
                mods |= match m.to_ascii_lowercase().as_str() {
                    "ctrl" | "control" => CTRL,
                    "alt" | "meta" => ALT,
                    "super" => SUPER,
                    "shift" => SHIFT,
                    _ => return Err(KeyParseError::Modifier(m.to_owned())),
                };
            }
        }
        let mut chars = key_str.chars();
        let key = match (chars.next(), chars.next()) {
            (Some(' '), None) => Key::Named(Named::Space),
            (Some(c), None) => Key::Char(c),
            _ => {
                let lower = key_str.to_ascii_lowercase();
                if lower == "plus" {
                    Key::Char('+')
                } else if lower == "backtab" {
                    mods |= SHIFT;
                    Key::Named(Named::Tab)
                } else if let Some((_, n)) = NAMES.iter().find(|(name, _)| *name == lower) {
                    Key::Named(*n)
                } else if let Some(n) = lower
                    .strip_prefix('f')
                    .and_then(|n| n.parse::<u8>().ok())
                    .filter(|n| (1..=24).contains(n))
                {
                    Key::Named(Named::F(n))
                } else {
                    return Err(KeyParseError::Name(key_str.to_owned()));
                }
            }
        };
        let key = match key {
            Key::Char(c) if mods & SHIFT != 0 => {
                if c.is_alphabetic() && c.to_uppercase().count() == 1 {
                    mods &= !SHIFT;
                    Key::Char(c.to_uppercase().next().unwrap())
                } else {
                    return Err(KeyParseError::ShiftedSymbol(s.to_owned()));
                }
            }
            k => k,
        };
        Ok(Self { mods, key })
    }

    /// Normalizes a crossterm key event. `None` for events that never map to
    /// a binding (bare modifiers, media keys).
    pub fn from_event(ev: &KeyEvent) -> Option<Self> {
        let mut mods = 0;
        let m = ev.modifiers;
        if m.contains(KeyModifiers::CONTROL) {
            mods |= CTRL;
        }
        // Meta is Alt in terminal parlance, matching `meta+` in the notation.
        if m.intersects(KeyModifiers::ALT | KeyModifiers::META) {
            mods |= ALT;
        }
        if m.intersects(KeyModifiers::SUPER | KeyModifiers::HYPER) {
            mods |= SUPER;
        }
        let shift = m.contains(KeyModifiers::SHIFT);
        let key = match ev.code {
            KeyCode::Char(' ') => {
                if shift {
                    mods |= SHIFT;
                }
                Key::Named(Named::Space)
            }
            KeyCode::Char(c) => {
                // Without alternate-key reporting a shifted letter can arrive
                // as lowercase + SHIFT; fold it like the terminal would.
                if shift && c.is_lowercase() && c.to_uppercase().count() == 1 {
                    Key::Char(c.to_uppercase().next().unwrap())
                } else {
                    Key::Char(c)
                }
            }
            KeyCode::BackTab => {
                mods |= SHIFT;
                Key::Named(Named::Tab)
            }
            code => {
                let named = match code {
                    KeyCode::Enter => Named::Enter,
                    KeyCode::Tab => Named::Tab,
                    KeyCode::Esc => Named::Esc,
                    KeyCode::Backspace => Named::Backspace,
                    KeyCode::Delete => Named::Delete,
                    KeyCode::Insert => Named::Insert,
                    KeyCode::Home => Named::Home,
                    KeyCode::End => Named::End,
                    KeyCode::PageUp => Named::PageUp,
                    KeyCode::PageDown => Named::PageDown,
                    KeyCode::Up => Named::Up,
                    KeyCode::Down => Named::Down,
                    KeyCode::Left => Named::Left,
                    KeyCode::Right => Named::Right,
                    KeyCode::F(n) if (1..=24).contains(&n) => Named::F(n),
                    _ => return None,
                };
                if shift {
                    mods |= SHIFT;
                }
                Key::Named(named)
            }
        };
        Some(Self { mods, key })
    }

    /// Maps a spec to what a legacy (non-KKP) terminal actually sends, e.g.
    /// `ctrl+[` arrives as `esc`, `ctrl+\` as `ctrl+4`.
    pub fn legacy_alias(self) -> Self {
        if self.mods & CTRL == 0 {
            return self;
        }
        let rest = self.mods & !CTRL;
        let to = |key| Self::new(rest, key);
        match self.key {
            Key::Char('[') => to(Key::Named(Named::Esc)),
            Key::Char('i') => to(Key::Named(Named::Tab)),
            Key::Char('m') => to(Key::Named(Named::Enter)),
            Key::Char('\\') => Self::new(self.mods, Key::Char('4')),
            Key::Char(']') => Self::new(self.mods, Key::Char('5')),
            Key::Char('^') | Key::Char('~') => Self::new(self.mods, Key::Char('6')),
            Key::Char('_') | Key::Char('/') => Self::new(self.mods, Key::Char('7')),
            Key::Char('@') | Key::Char('2') => Self::new(self.mods, Key::Named(Named::Space)),
            _ => self,
        }
    }

    /// Whether a legacy terminal can send this chord at all.
    pub fn legacy_reachable(self) -> bool {
        let s = self.legacy_alias();
        if s.mods & SUPER != 0 {
            return false;
        }
        let ctrl = s.mods & CTRL != 0;
        let shift = s.mods & SHIFT != 0;
        match s.key {
            Key::Char(c) => !ctrl || c.is_ascii_lowercase() || ('4'..='7').contains(&c) || c == '?',
            Key::Named(n) => match n {
                Named::Enter | Named::Esc | Named::Backspace => !ctrl && !shift,
                Named::Space => !shift,
                Named::Tab => !ctrl,
                _ => true,
            },
        }
    }
}

impl fmt::Display for KeySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods & CTRL != 0 {
            f.write_str("ctrl+")?;
        }
        if self.mods & ALT != 0 {
            f.write_str("alt+")?;
        }
        if self.mods & SUPER != 0 {
            f.write_str("super+")?;
        }
        if self.mods & SHIFT != 0 {
            f.write_str("shift+")?;
        }
        match self.key {
            Key::Char('+') => f.write_str("plus"),
            Key::Char(c) => write!(f, "{c}"),
            Key::Named(Named::F(n)) => write!(f, "f{n}"),
            Key::Named(n) => {
                let name = NAMES.iter().find(|(_, k)| *k == n).map(|(s, _)| *s);
                f.write_str(name.unwrap_or("?"))
            }
        }
    }
}

/// Translates a crossterm event into the key rime expects. Release events
/// carry the Release mask; repeats are presses. Bare modifier keys (only
/// reported under KKP) follow X11: the key's own modifier bit is absent on
/// press and present on release.
pub fn to_rime(ev: &KeyEvent) -> Option<RimeKey> {
    let m = ev.modifiers;
    let mut bits = 0;
    if m.contains(KeyModifiers::SHIFT) {
        bits |= mask::SHIFT;
    }
    if m.contains(KeyModifiers::CONTROL) {
        bits |= mask::CONTROL;
    }
    if m.contains(KeyModifiers::ALT) {
        bits |= mask::ALT;
    }
    if m.contains(KeyModifiers::SUPER) {
        bits |= mask::SUPER;
    }
    if m.contains(KeyModifiers::HYPER) {
        bits |= mask::HYPER;
    }
    if m.contains(KeyModifiers::META) {
        bits |= mask::META;
    }
    let keycode = match ev.code {
        KeyCode::Char(c) => {
            if c.is_uppercase() {
                bits |= mask::SHIFT;
            } else if c.is_lowercase() && bits & mask::SHIFT != 0 {
                return Some(finish(ev, keycode_for_char(c.to_uppercase().next()?), bits));
            }
            keycode_for_char(c)
        }
        KeyCode::Enter => sym::RETURN,
        KeyCode::Tab => sym::TAB,
        KeyCode::BackTab => {
            bits |= mask::SHIFT;
            sym::ISO_LEFT_TAB
        }
        KeyCode::Esc => sym::ESCAPE,
        KeyCode::Backspace => sym::BACKSPACE,
        KeyCode::Delete => sym::DELETE,
        KeyCode::Insert => sym::INSERT,
        KeyCode::Home => sym::HOME,
        KeyCode::End => sym::END,
        KeyCode::PageUp => sym::PAGE_UP,
        KeyCode::PageDown => sym::PAGE_DOWN,
        KeyCode::Up => sym::UP,
        KeyCode::Down => sym::DOWN,
        KeyCode::Left => sym::LEFT,
        KeyCode::Right => sym::RIGHT,
        KeyCode::F(n) if (1..=35).contains(&n) => sym::F1 + n as u32 - 1,
        KeyCode::CapsLock => sym::CAPS_LOCK,
        KeyCode::Menu => sym::MENU,
        KeyCode::Modifier(mk) => {
            let (code, own) = match mk {
                ModifierKeyCode::LeftShift => (sym::SHIFT_L, mask::SHIFT),
                ModifierKeyCode::RightShift => (sym::SHIFT_R, mask::SHIFT),
                ModifierKeyCode::LeftControl => (sym::CONTROL_L, mask::CONTROL),
                ModifierKeyCode::RightControl => (sym::CONTROL_R, mask::CONTROL),
                ModifierKeyCode::LeftAlt => (sym::ALT_L, mask::ALT),
                ModifierKeyCode::RightAlt => (sym::ALT_R, mask::ALT),
                ModifierKeyCode::LeftSuper => (sym::SUPER_L, mask::SUPER),
                ModifierKeyCode::RightSuper => (sym::SUPER_R, mask::SUPER),
                ModifierKeyCode::LeftHyper => (sym::HYPER_L, mask::HYPER),
                ModifierKeyCode::RightHyper => (sym::HYPER_R, mask::HYPER),
                ModifierKeyCode::LeftMeta => (sym::META_L, mask::META),
                ModifierKeyCode::RightMeta => (sym::META_R, mask::META),
                _ => return None,
            };
            if matches!(ev.kind, KeyEventKind::Release) {
                bits |= own;
            } else {
                bits &= !own;
            }
            code
        }
        _ => return None,
    };
    Some(finish(ev, keycode, bits))
}

fn finish(ev: &KeyEvent, keycode: u32, bits: u32) -> RimeKey {
    let bits = if matches!(ev.kind, KeyEventKind::Release) {
        bits | mask::RELEASE
    } else {
        bits
    };
    RimeKey::new(keycode, bits)
}

/// The key sequence sent for a compat binding: a press followed by its
/// release. For a bare modifier such as `Shift_L` this is the "tap" that
/// `ascii_composer` listens for.
pub fn compat_sequence(target: RimeKey) -> [RimeKey; 2] {
    let press = RimeKey::new(target.keycode, target.mask & !mask::RELEASE);
    [press, press.released()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    fn p(s: &str) -> KeySpec {
        KeySpec::parse(s).unwrap()
    }

    fn ev(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    #[test]
    fn parse_basics() {
        assert_eq!(p("alt+l"), KeySpec::new(ALT, Key::Char('l')));
        assert_eq!(p("alt+K"), KeySpec::new(ALT, Key::Char('K')));
        assert_ne!(p("alt+K"), p("alt+k"));
        assert_eq!(p("shift+alt+k"), p("alt+K"));
        assert_ne!(p("alt+:"), p("alt+;"));
        assert_eq!(p("ctrl+,"), KeySpec::new(CTRL, Key::Char(',')));
        assert_eq!(p("ctrl+["), KeySpec::new(CTRL, Key::Char('[')));
        assert_eq!(p("ctrl+plus"), KeySpec::new(CTRL, Key::Char('+')));
        assert_eq!(p("ctrl++"), p("ctrl+plus"));
        assert_eq!(p("+"), KeySpec::new(0, Key::Char('+')));
        assert_eq!(
            p("CTRL+Enter"),
            KeySpec::new(CTRL, Key::Named(Named::Enter))
        );
        assert_eq!(p("backtab"), p("shift+tab"));
        assert_eq!(p("shift+f1"), KeySpec::new(SHIFT, Key::Named(Named::F(1))));
        assert_eq!(p("f24"), KeySpec::new(0, Key::Named(Named::F(24))));
        assert_eq!(p("space"), p(" "));
    }

    #[test]
    fn parse_errors() {
        assert!(matches!(
            KeySpec::parse("shift+;"),
            Err(KeyParseError::ShiftedSymbol(_))
        ));
        assert!(matches!(
            KeySpec::parse("hyper+a"),
            Err(KeyParseError::Modifier(_))
        ));
        assert!(matches!(
            KeySpec::parse("ctrl+foo"),
            Err(KeyParseError::Name(_))
        ));
        assert!(matches!(KeySpec::parse("f25"), Err(KeyParseError::Name(_))));
        assert!(KeySpec::parse("").is_err());
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "alt+K",
            "ctrl+,",
            "ctrl+plus",
            "shift+tab",
            "f5",
            "ctrl+alt+x",
            "space",
        ] {
            assert_eq!(p(s).to_string(), s);
            assert_eq!(p(&p(s).to_string()), p(s));
        }
    }

    #[test]
    fn events_normalize() {
        // Legacy alt+K: ESC K.
        let e = ev(KeyCode::Char('K'), KeyModifiers::ALT);
        assert_eq!(KeySpec::from_event(&e), Some(p("alt+K")));
        // KKP without alternate keys: k + SHIFT + ALT.
        let e = ev(KeyCode::Char('k'), KeyModifiers::ALT | KeyModifiers::SHIFT);
        assert_eq!(KeySpec::from_event(&e), Some(p("alt+K")));
        // Shifted symbol arrives as the character; SHIFT is dropped.
        let e = ev(KeyCode::Char(':'), KeyModifiers::ALT | KeyModifiers::SHIFT);
        assert_eq!(KeySpec::from_event(&e), Some(p("alt+:")));
        let e = ev(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(KeySpec::from_event(&e), Some(p("shift+tab")));
        let e = ev(KeyCode::Char('j'), KeyModifiers::CONTROL);
        assert_eq!(KeySpec::from_event(&e), Some(p("ctrl+j")));
        let e = ev(
            KeyCode::Modifier(ModifierKeyCode::LeftShift),
            KeyModifiers::SHIFT,
        );
        assert_eq!(KeySpec::from_event(&e), None);
    }

    #[test]
    fn legacy_aliases() {
        assert_eq!(p("ctrl+[").legacy_alias(), p("esc"));
        assert_eq!(p("ctrl+alt+[").legacy_alias(), p("alt+esc"));
        assert_eq!(p("ctrl+i").legacy_alias(), p("tab"));
        assert_eq!(p("ctrl+m").legacy_alias(), p("enter"));
        assert_eq!(p("ctrl+\\").legacy_alias(), p("ctrl+4"));
        assert_eq!(p("ctrl+j").legacy_alias(), p("ctrl+j"));
        assert_eq!(p("ctrl+h").legacy_alias(), p("ctrl+h"));
    }

    #[test]
    fn legacy_reachability() {
        for s in [
            "ctrl+a",
            "alt+K",
            "alt+:",
            "ctrl+[",
            "shift+tab",
            "ctrl+left",
            "f5",
            "ctrl+j",
        ] {
            assert!(p(s).legacy_reachable(), "{s}");
        }
        for s in [
            "ctrl+,",
            "ctrl+;",
            "ctrl+1",
            "ctrl+K",
            "shift+enter",
            "super+a",
            "ctrl+enter",
        ] {
            assert!(!p(s).legacy_reachable(), "{s}");
        }
    }

    #[test]
    fn rime_translation() {
        let r = |e: KeyEvent| to_rime(&e).unwrap();
        assert_eq!(
            r(ev(KeyCode::Char('a'), KeyModifiers::NONE)),
            RimeKey::new(0x61, 0)
        );
        assert_eq!(
            r(ev(KeyCode::Char('A'), KeyModifiers::NONE)),
            RimeKey::new(0x41, mask::SHIFT)
        );
        assert_eq!(
            r(ev(KeyCode::Char('a'), KeyModifiers::SHIFT)),
            RimeKey::new(0x41, mask::SHIFT)
        );
        assert_eq!(
            r(ev(KeyCode::Char(':'), KeyModifiers::ALT)),
            RimeKey::new(0x3a, mask::ALT)
        );
        assert_eq!(
            r(ev(KeyCode::Char('`'), KeyModifiers::CONTROL)),
            RimeKey::new(0x60, mask::CONTROL)
        );
        assert_eq!(
            r(ev(KeyCode::BackTab, KeyModifiers::SHIFT)),
            RimeKey::new(sym::ISO_LEFT_TAB, mask::SHIFT)
        );
        let press = KeyEvent::new_with_kind_and_state(
            KeyCode::Modifier(ModifierKeyCode::LeftShift),
            KeyModifiers::SHIFT,
            KeyEventKind::Press,
            KeyEventState::NONE,
        );
        assert_eq!(r(press), RimeKey::new(sym::SHIFT_L, 0));
        let release = KeyEvent::new_with_kind_and_state(
            KeyCode::Modifier(ModifierKeyCode::LeftShift),
            KeyModifiers::SHIFT,
            KeyEventKind::Release,
            KeyEventState::NONE,
        );
        assert_eq!(
            r(release),
            RimeKey::new(sym::SHIFT_L, mask::SHIFT | mask::RELEASE)
        );
    }

    #[test]
    fn compat_taps() {
        let [a, b] = compat_sequence(RimeKey::parse("Shift_L").unwrap());
        assert_eq!(a, RimeKey::new(sym::SHIFT_L, 0));
        assert_eq!(b, RimeKey::new(sym::SHIFT_L, mask::RELEASE));
    }
}
