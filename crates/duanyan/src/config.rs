//! `config.toml`: defaults, merging, and keymap construction.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rime_dl::RimeKey;
use serde::Deserialize;

use crate::keys::KeySpec;

pub const DEFAULT_CONFIG: &str = include_str!("default_config.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub rime: RimeSection,
    pub tui: Tui,
    pub theme: ThemeSection,
    pub clipboard: ClipboardSection,
    pub history: HistorySection,
    pub keybinding: KeybindingSection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct General {
    pub copy_on_submit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeployOnStartup {
    Notify,
    Auto,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

impl LogLevel {
    /// glog's `minloglevel`.
    pub fn glog(self) -> i32 {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Error => 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RimeSection {
    pub librime_path: Option<PathBuf>,
    pub shared_data_dir: Option<PathBuf>,
    pub user_data_dir: Option<PathBuf>,
    pub deploy_on_startup: DeployOnStartup,
    pub log_level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tristate {
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateLayout {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tui {
    pub kitty_keyboard: Tristate,
    pub mouse: bool,
    pub candidate_layout: CandidateLayout,
    pub show_candidate_comment: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Auto,
    Dark,
    Light,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSection {
    pub mode: ThemeMode,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardBackend {
    Osc52,
    Command,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipboardSection {
    pub backend: ClipboardBackend,
    pub command: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistorySection {
    pub persist: bool,
    pub max_entries: usize,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Keys {
    One(String),
    Many(Vec<String>),
}

impl Keys {
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        let v: Vec<&str> = match self {
            Self::One(s) => vec![s.as_str()],
            Self::Many(v) => v.iter().map(String::as_str).collect(),
        };
        v.into_iter()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeybindingSection {
    pub global: BTreeMap<String, Keys>,
    pub compat: BTreeMap<String, Keys>,
    pub input: BTreeMap<String, Keys>,
    pub history: BTreeMap<String, Keys>,
}

impl Config {
    pub fn default_config() -> Self {
        Self::from_str("").expect("built-in defaults are valid")
    }

    /// Parses user TOML merged over the defaults.
    pub fn from_str(user: &str) -> anyhow::Result<Self> {
        let mut base: toml::Table = DEFAULT_CONFIG.parse().expect("built-in defaults parse");
        let user: toml::Table = user.parse()?;
        merge(&mut base, user, 0);
        Ok(toml::Value::Table(base).try_into()?)
    }

    /// Loads `path`; a missing file yields the defaults.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                Self::from_str(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default_config()),
            Err(e) => Err(anyhow::anyhow!("{}: {e}", path.display())),
        }
    }
}

/// Merges `over` into `base`. Tables merge recursively, except that inside
/// `[keybinding.*]` each action's value is replaced as a whole.
fn merge(base: &mut toml::Table, over: toml::Table, depth: usize) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) if depth < 2 => {
                merge(b, o, depth + 1)
            }
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

macro_rules! actions {
    ($name:ident { $($variant:ident = $s:literal),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),* }

        impl $name {
            pub const ALL: &[Self] = &[$(Self::$variant),*];

            pub fn name(self) -> &'static str {
                match self { $(Self::$variant => $s),* }
            }

            pub fn from_name(s: &str) -> Option<Self> {
                match s { $($s => Some(Self::$variant),)* _ => None }
            }
        }
    };
}

actions!(GlobalAction {
    Quit = "quit",
    Help = "help",
    Deploy = "deploy",
    Sync = "sync",
});

actions!(InputAction {
    Submit = "submit",
    Newline = "newline",
    FocusHistory = "focus_history",
    Cancel = "cancel",
    Left = "left",
    Right = "right",
    PrevLine = "prev_line",
    NextLine = "next_line",
    Home = "home",
    End = "end",
    Backspace = "backspace",
    Delete = "delete",
    KillWord = "kill_word",
    KillToStart = "kill_to_start",
    KillToEnd = "kill_to_end",
});

actions!(HistoryAction {
    Next = "next",
    Prev = "prev",
    First = "first",
    Last = "last",
    Copy = "copy",
    Recall = "recall",
    Delete = "delete",
    FocusInput = "focus_input",
});

/// Resolved bindings for the current terminal mode.
#[derive(Debug, Default)]
pub struct Keymap {
    pub global: HashMap<KeySpec, GlobalAction>,
    pub compat: HashMap<KeySpec, RimeKey>,
    pub input: HashMap<KeySpec, InputAction>,
    pub history: HashMap<KeySpec, HistoryAction>,
    /// Bindings as written, for the help screen: (table, action, keys).
    pub listing: Vec<(&'static str, String, Vec<KeySpec>)>,
    pub kkp: bool,
}

impl Keymap {
    /// Builds the lookup tables. Without KKP, specs are mapped to what a
    /// legacy terminal sends (`ctrl+[` -> `esc`) before conflicts are checked.
    pub fn build(kb: &KeybindingSection, kkp: bool) -> Result<Self, Vec<String>> {
        let mut errors = Vec::new();
        let mut map = Keymap {
            kkp,
            ..Default::default()
        };
        map.global = table(
            "global",
            &kb.global,
            kkp,
            GlobalAction::from_name,
            &mut map.listing,
            &mut errors,
        );
        map.compat = table(
            "compat",
            &kb.compat,
            kkp,
            |s| RimeKey::parse(s).ok(),
            &mut map.listing,
            &mut errors,
        );
        map.input = table(
            "input",
            &kb.input,
            kkp,
            InputAction::from_name,
            &mut map.listing,
            &mut errors,
        );
        map.history = table(
            "history",
            &kb.history,
            kkp,
            HistoryAction::from_name,
            &mut map.listing,
            &mut errors,
        );
        map.listing
            .sort_by_key(|(table, action, _)| action_rank(table, action));
        if errors.is_empty() {
            Ok(map)
        } else {
            Err(errors)
        }
    }

    /// Keys bound to `action` in `table`, as written in the config.
    pub fn keys_for(&self, table: &str, action: &str) -> Vec<KeySpec> {
        self.listing
            .iter()
            .find(|(t, a, _)| *t == table && a == action)
            .map(|(_, _, k)| k.clone())
            .unwrap_or_default()
    }
}

/// Position of an action in its enum, so the help screen follows the order
/// of the default config rather than alphabetical order.
fn action_rank(table: &str, action: &str) -> (usize, usize) {
    let names: Vec<&str> = match table {
        "global" => GlobalAction::ALL.iter().map(|a| a.name()).collect(),
        "input" => InputAction::ALL.iter().map(|a| a.name()).collect(),
        "history" => HistoryAction::ALL.iter().map(|a| a.name()).collect(),
        _ => Vec::new(),
    };
    let t = ["global", "compat", "input", "history"]
        .iter()
        .position(|x| *x == table)
        .unwrap_or(usize::MAX);
    (t, names.iter().position(|n| *n == action).unwrap_or(0))
}

fn table<A: Copy + std::fmt::Debug>(
    name: &'static str,
    entries: &BTreeMap<String, Keys>,
    kkp: bool,
    resolve: impl Fn(&str) -> Option<A>,
    listing: &mut Vec<(&'static str, String, Vec<KeySpec>)>,
    errors: &mut Vec<String>,
) -> HashMap<KeySpec, A> {
    let mut out: HashMap<KeySpec, (A, String)> = HashMap::new();
    for (action_name, keys) in entries {
        let Some(action) = resolve(action_name) else {
            errors.push(if name == "compat" {
                format!("keybinding.compat: invalid rime key {action_name:?}")
            } else {
                format!("keybinding.{name}: unknown action {action_name:?}")
            });
            continue;
        };
        let mut specs = Vec::new();
        for k in keys.iter() {
            let spec = match KeySpec::parse(k) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("keybinding.{name}.{action_name}: {e}"));
                    continue;
                }
            };
            specs.push(spec);
            let effective = if kkp { spec } else { spec.legacy_alias() };
            if let Some((_, other)) = out.get(&effective) {
                if other != action_name {
                    let note = if effective != spec {
                        format!(
                            " (without the kitty keyboard protocol {spec} is sent as {effective})"
                        )
                    } else {
                        String::new()
                    };
                    errors.push(format!(
                        "keybinding.{name}: {spec} is bound to both {other:?} and {action_name:?}{note}"
                    ));
                }
                continue;
            }
            out.insert(effective, (action, action_name.clone()));
        }
        listing.push((name, action_name.clone(), specs));
    }
    out.into_iter().map(|(k, (a, _))| (k, a)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Key, Named};

    #[test]
    fn defaults_build() {
        let cfg = Config::default_config();
        assert!(cfg.general.copy_on_submit);
        assert_eq!(cfg.rime.deploy_on_startup, DeployOnStartup::Notify);
        for kkp in [false, true] {
            let km = Keymap::build(&cfg.keybinding, kkp).unwrap();
            assert_eq!(
                km.input.get(&KeySpec::parse("enter").unwrap()),
                Some(&InputAction::Submit)
            );
            assert_eq!(
                km.compat.get(&KeySpec::parse("alt+l").unwrap()),
                Some(&RimeKey::parse("Shift_L").unwrap())
            );
            assert_eq!(
                km.history.get(&KeySpec::new(0, Key::Char('G'))),
                Some(&HistoryAction::Last)
            );
        }
        // Every action has a default binding.
        for a in InputAction::ALL {
            assert!(cfg.keybinding.input.contains_key(a.name()), "{}", a.name());
        }
        for a in HistoryAction::ALL {
            assert!(
                cfg.keybinding.history.contains_key(a.name()),
                "{}",
                a.name()
            );
        }
        for a in GlobalAction::ALL {
            assert!(cfg.keybinding.global.contains_key(a.name()), "{}", a.name());
        }
    }

    #[test]
    fn user_overrides_merge_per_action() {
        let cfg = Config::from_str(
            r#"
            [general]
            copy_on_submit = false
            [keybinding.input]
            submit = "alt+enter"
            newline = []
            [keybinding.compat]
            Shift_L = ["alt+l", "f9"]
            "#,
        )
        .unwrap();
        assert!(!cfg.general.copy_on_submit);
        // Untouched sections keep defaults.
        assert!(cfg.history.persist);
        let km = Keymap::build(&cfg.keybinding, true).unwrap();
        let enter = KeySpec::new(0, Key::Named(Named::Enter));
        assert_eq!(km.input.get(&enter), None);
        assert_eq!(
            km.input.get(&KeySpec::parse("alt+enter").unwrap()),
            Some(&InputAction::Submit)
        );
        assert_eq!(km.input.get(&KeySpec::parse("ctrl+j").unwrap()), None);
        assert!(km.compat.contains_key(&KeySpec::parse("f9").unwrap()));
        // Other input actions keep their defaults.
        assert_eq!(
            km.input.get(&KeySpec::parse("ctrl+a").unwrap()),
            Some(&InputAction::Home)
        );
    }

    #[test]
    fn validation_errors() {
        let cfg = Config::from_str(
            r#"
            [keybinding.input]
            frobnicate = "f2"
            submit = "shift+;"
            [keybinding.compat]
            Shift_X = "alt+x"
            "#,
        )
        .unwrap();
        let errs = Keymap::build(&cfg.keybinding, true).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("frobnicate")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("shift+;")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("Shift_X")), "{errs:?}");
    }

    #[test]
    fn legacy_conflicts() {
        // ctrl+[ and esc are the same key without KKP.
        let cfg = Config::from_str(
            r#"
            [keybinding.input]
            kill_word = "ctrl+["
            "#,
        )
        .unwrap();
        assert!(Keymap::build(&cfg.keybinding, true).is_ok());
        let errs = Keymap::build(&cfg.keybinding, false).unwrap_err();
        assert!(errs[0].contains("ctrl+["), "{errs:?}");
    }

    #[test]
    fn unknown_fields_rejected() {
        assert!(Config::from_str("[tui]\nmouse_mode = true\n").is_err());
        assert!(Config::from_str("[rime]\ndeploy_on_startup = \"sometimes\"\n").is_err());
    }
}
