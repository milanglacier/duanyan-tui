//! The input-method engine seam. The UI talks to [`ImeEngine`] only and sees
//! owned snapshots, so a daemon-backed engine can replace [`RimeEngine`]
//! later without touching the UI.

use std::path::Path;
use std::sync::mpsc;
use std::time::SystemTime;

use rime_dl::{Notification, Rime, RimeKey, Session};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preedit {
    pub text: String,
    /// Byte offsets into `text`.
    pub cursor: usize,
    pub sel_start: usize,
    pub sel_end: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidate {
    pub label: String,
    pub text: String,
    pub comment: Option<String>,
}

/// One entry of the schema's `switches`, with its current state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwitchView {
    /// Option names; more than one for a radio group.
    pub options: Vec<String>,
    pub label: String,
    pub is_ascii_mode: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImeSnapshot {
    pub composing: bool,
    pub preedit: Option<Preedit>,
    pub raw_input: Option<String>,
    pub candidates: Vec<Candidate>,
    pub highlighted: usize,
    pub page_no: usize,
    pub is_last_page: bool,
    pub schema_id: String,
    pub schema_name: String,
    pub ascii_mode: bool,
    pub switches: Vec<SwitchView>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyOutcome {
    pub handled: bool,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maintenance {
    Deploy,
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    Finished { kind: Maintenance, ok: bool },
    SchemaChanged,
}

pub trait ImeEngine {
    fn process_key(&mut self, key: RimeKey) -> KeyOutcome;
    fn snapshot(&self) -> ImeSnapshot;
    /// Selects a candidate on the current page; returns committed text.
    fn select_candidate(&mut self, index: usize) -> Option<String>;
    fn change_page(&mut self, backward: bool);
    fn toggle_switch(&mut self, index: usize);
    fn clear_composition(&mut self);
    /// Starts an asynchronous full deploy.
    fn start_deploy(&mut self) -> bool;
    fn start_sync(&mut self) -> bool;
    fn busy(&self) -> Option<Maintenance>;
    /// Drains notifications and completes maintenance; call every tick.
    fn poll(&mut self) -> Vec<EngineEvent>;
}

/// [`ImeEngine`] backed by an in-process librime.
pub struct RimeEngine {
    rime: Rime,
    session: Option<Session>,
    notifications: mpsc::Receiver<Notification>,
    busy: Option<Maintenance>,
    deploy_failed: bool,
    switches: Vec<SwitchDef>,
}

#[derive(Debug, Clone)]
struct SwitchDef {
    options: Vec<String>,
    states: Vec<String>,
}

impl RimeEngine {
    /// `rime` must have been initialized with a handler that forwards into
    /// the sender paired with `notifications`.
    pub fn new(rime: Rime, notifications: mpsc::Receiver<Notification>) -> Self {
        Self {
            rime,
            session: None,
            notifications,
            busy: None,
            deploy_failed: false,
            switches: Vec::new(),
        }
    }

    /// Opens the session once no maintenance is running.
    pub fn open_session(&mut self) -> anyhow::Result<()> {
        let session = self.rime.create_session()?;
        self.session = Some(session);
        self.load_switches();
        Ok(())
    }

    fn load_switches(&mut self) {
        self.switches.clear();
        let Some(schema) = self.session.as_ref().and_then(|s| s.current_schema()) else {
            return;
        };
        let Some(mut cfg) = self.rime.schema_config(&schema) else {
            return;
        };
        for i in 0..cfg.list_size("switches") {
            let base = format!("switches/@{i}");
            let options = match cfg.get_string(&format!("{base}/name")) {
                Some(name) => vec![name],
                None => (0..cfg.list_size(&format!("{base}/options")))
                    .filter_map(|j| cfg.get_string(&format!("{base}/options/@{j}")))
                    .collect(),
            };
            let states: Vec<String> = (0..cfg.list_size(&format!("{base}/states")))
                .filter_map(|j| cfg.get_string(&format!("{base}/states/@{j}")))
                .collect();
            // Switches without states are hidden by convention.
            if !options.is_empty() && !states.is_empty() {
                self.switches.push(SwitchDef { options, states });
            }
        }
    }

    fn switch_state(session: &Session, def: &SwitchDef) -> usize {
        if def.options.len() == 1 {
            session.get_option(&def.options[0]) as usize
        } else {
            def.options
                .iter()
                .position(|o| session.get_option(o))
                .unwrap_or(0)
        }
    }

    pub fn last_build_time(&self) -> i64 {
        last_build_time(&self.rime)
    }

    /// Candidates per page: the schema's `menu/page_size`, else `default`'s.
    pub fn page_size(&self) -> usize {
        let from_schema = self
            .session
            .as_ref()
            .and_then(|s| s.current_schema())
            .and_then(|id| self.rime.schema_config(&id))
            .and_then(|mut c| c.get_int("menu/page_size"));
        from_schema
            .or_else(|| {
                self.rime
                    .config("default")
                    .and_then(|mut c| c.get_int("menu/page_size"))
            })
            .filter(|n| *n > 0)
            .unwrap_or(5) as usize
    }
}

/// Reads `var/last_build_time` from `user.yaml`.
pub fn last_build_time(rime: &Rime) -> i64 {
    rime.user_config("user")
        .and_then(|mut c| c.get_int("var/last_build_time"))
        .unwrap_or(0) as i64
}

impl ImeEngine for RimeEngine {
    fn process_key(&mut self, key: RimeKey) -> KeyOutcome {
        let Some(s) = &self.session else {
            return KeyOutcome::default();
        };
        let handled = s.process_key(key);
        KeyOutcome {
            handled,
            commit: s.take_commit(),
        }
    }

    fn snapshot(&self) -> ImeSnapshot {
        let Some(s) = &self.session else {
            return ImeSnapshot::default();
        };
        let ctx = s.context().unwrap_or_default();
        let status = s.status().unwrap_or_default();
        let labels = &ctx.select_labels;
        let keys: Vec<char> = ctx
            .menu
            .select_keys
            .as_deref()
            .unwrap_or("")
            .chars()
            .collect();
        let candidates = ctx
            .menu
            .candidates
            .into_iter()
            .enumerate()
            .map(|(i, c)| Candidate {
                label: labels
                    .get(i)
                    .cloned()
                    .or_else(|| keys.get(i).map(char::to_string))
                    .unwrap_or_else(|| ((i + 1) % 10).to_string()),
                text: c.text,
                comment: c.comment,
            })
            .collect();
        let switches = self
            .switches
            .iter()
            .map(|def| {
                let state = Self::switch_state(s, def);
                let is_ascii_mode = def.options[0] == "ascii_mode";
                let label = if is_ascii_mode {
                    s.state_label("ascii_mode", state == 1, true)
                } else {
                    None
                }
                .or_else(|| def.states.get(state).cloned())
                .unwrap_or_default();
                SwitchView {
                    options: def.options.clone(),
                    label,
                    is_ascii_mode,
                }
            })
            .collect();
        ImeSnapshot {
            composing: status.is_composing,
            preedit: ctx.composition.map(|c| Preedit {
                text: c.preedit,
                cursor: c.cursor_pos,
                sel_start: c.sel_start,
                sel_end: c.sel_end,
            }),
            raw_input: s.input().filter(|i| !i.is_empty()),
            candidates,
            highlighted: ctx.menu.highlighted,
            page_no: ctx.menu.page_no,
            is_last_page: ctx.menu.is_last_page,
            schema_id: status.schema_id,
            schema_name: status.schema_name,
            ascii_mode: status.is_ascii_mode,
            switches,
        }
    }

    fn select_candidate(&mut self, index: usize) -> Option<String> {
        let s = self.session.as_ref()?;
        s.select_candidate_on_current_page(index);
        s.take_commit()
    }

    fn change_page(&mut self, backward: bool) {
        let Some(s) = &self.session else { return };
        if s.change_page(backward).is_none() {
            let code = if backward {
                rime_dl::keysym::sym::PAGE_UP
            } else {
                rime_dl::keysym::sym::PAGE_DOWN
            };
            s.process_key(RimeKey::new(code, 0));
        }
    }

    fn toggle_switch(&mut self, index: usize) {
        let (Some(s), Some(def)) = (&self.session, self.switches.get(index)) else {
            return;
        };
        if def.options.len() == 1 {
            let o = &def.options[0];
            s.set_option(o, !s.get_option(o));
        } else {
            let cur = Self::switch_state(s, def);
            let next = (cur + 1) % def.options.len();
            for (i, o) in def.options.iter().enumerate() {
                s.set_option(o, i == next);
            }
        }
    }

    fn clear_composition(&mut self) {
        if let Some(s) = &self.session {
            s.clear_composition();
        }
    }

    fn start_deploy(&mut self) -> bool {
        if self.busy.is_some() {
            return false;
        }
        self.session = None;
        self.deploy_failed = false;
        if self.rime.start_maintenance(true) {
            self.busy = Some(Maintenance::Deploy);
            true
        } else {
            let _ = self.open_session();
            false
        }
    }

    fn start_sync(&mut self) -> bool {
        if self.busy.is_some() {
            return false;
        }
        // Sync destroys all sessions inside librime anyway.
        self.session = None;
        self.deploy_failed = false;
        if self.rime.sync_user_data() {
            self.busy = Some(Maintenance::Sync);
            true
        } else {
            let _ = self.open_session();
            false
        }
    }

    fn busy(&self) -> Option<Maintenance> {
        self.busy
    }

    fn poll(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        while let Ok(n) = self.notifications.try_recv() {
            match (n.message_type.as_str(), n.message_value.as_str()) {
                ("deploy", "failure") => self.deploy_failed = true,
                ("schema", _) => {
                    self.load_switches();
                    events.push(EngineEvent::SchemaChanged);
                }
                _ => {}
            }
        }
        if let Some(kind) = self.busy
            && !self.rime.is_maintenance_mode()
        {
            self.rime.join_maintenance_thread();
            self.busy = None;
            let ok = !self.deploy_failed && self.open_session().is_ok();
            events.push(EngineEvent::Finished { kind, ok });
        }
        events
    }
}

/// Whether the user data dir has never been deployed.
pub fn is_first_run(user_data_dir: &Path) -> bool {
    let Ok(dir) = std::fs::read_dir(user_data_dir.join("build")) else {
        return true;
    };
    !dir.flatten()
        .any(|e| e.file_name().to_string_lossy().ends_with(".schema.yaml"))
}

/// Like librime's `detect_modifications`, but ignoring directory mtimes,
/// which change whenever librime itself creates an entry (e.g. a userdb).
pub fn needs_deploy(dirs: &[&Path], last_build_time: i64) -> bool {
    let mut newest = 0i64;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".yaml") || name == "user.yaml" {
                continue;
            }
            // Follow symlinks, as nix-managed data dirs are symlink farms.
            let Ok(meta) = std::fs::metadata(e.path()) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            if let Ok(t) = meta.modified() {
                let secs = t
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as i64);
                newest = newest.max(secs);
            }
        }
    }
    newest > last_build_time
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modification_check() {
        let dir = std::env::temp_dir().join(format!("duanyan-mod-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        assert!(is_first_run(&dir));
        std::fs::write(dir.join("user.yaml"), "x").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // user.yaml and non-yaml files are ignored.
        assert!(!needs_deploy(&[&dir], 0));
        std::fs::write(dir.join("default.custom.yaml"), "x").unwrap();
        assert!(needs_deploy(&[&dir], now - 10));
        assert!(!needs_deploy(&[&dir], now + 10));
        std::fs::create_dir_all(dir.join("build")).unwrap();
        std::fs::write(dir.join("build/luna_pinyin.schema.yaml"), "x").unwrap();
        assert!(!is_first_run(&dir));
        std::fs::remove_dir_all(dir).ok();
    }
}
