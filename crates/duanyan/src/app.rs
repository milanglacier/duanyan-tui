//! Application state and key routing: global -> compat -> rime -> focus table.

use std::time::{Duration, Instant};

use crossterm::event::{
    KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};

use crate::buffer::Buffer;
use crate::config::{GlobalAction, HistoryAction, InputAction, Keymap};
use crate::engine::{EngineEvent, ImeEngine, ImeSnapshot, Maintenance};
use crate::history::History;
use crate::keys::{self, KeySpec};

const MESSAGE_TTL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Input,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Submitted text goes to the history and the clipboard.
    Scratch,
    /// `--stdout`: submitting prints the text and exits.
    Stdout,
    /// `duanyan FILE`: submitting saves the file and exits.
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    /// `--stdout`: print this text.
    Submit(String),
    /// The edited file was written.
    Saved,
    Cancel,
    Quit,
}

/// Side effects the event loop performs with terminal or file access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Copy(String),
    /// Write the edited file; on success the event loop sets `Exit::Saved`.
    Save(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub text: String,
    pub level: Level,
    pub at: Instant,
}

/// Clickable regions recorded while drawing.
#[derive(Debug, Default, Clone)]
pub struct HitMap {
    pub candidates: Vec<(Rect, usize)>,
    pub prev_page: Option<Rect>,
    pub next_page: Option<Rect>,
    pub switches: Vec<(Rect, usize)>,
    pub history_rows: Vec<(Rect, usize)>,
    pub history_area: Option<Rect>,
    pub input_area: Option<Rect>,
}

pub struct App<E: ImeEngine> {
    pub engine: E,
    pub keymap: Keymap,
    pub buffer: Buffer,
    pub history: History,
    pub focus: Focus,
    pub history_sel: usize,
    pub snapshot: ImeSnapshot,
    pub message: Option<Message>,
    /// History index copied last, with when.
    pub copied: Option<(usize, Instant)>,
    pub show_help: bool,
    pub help_scroll: u16,
    pub mode: Mode,
    /// `Mode::Edit`: the file as loaded, to tell whether it was modified.
    pub edit_original: String,
    /// When cancel was last pressed on a modified file; a second press
    /// within `MESSAGE_TTL` discards the changes.
    discard_armed: Option<Instant>,
    pub copy_on_submit: bool,
    /// Config changed since the last deploy (`deploy_on_startup = notify`).
    pub deploy_hint: bool,
    /// Another instance owns the user dictionary.
    pub secondary: bool,
    pub exit: Option<Exit>,
    pub effects: Vec<Effect>,
    pub hits: HitMap,
}

impl<E: ImeEngine> App<E> {
    pub fn new(engine: E, keymap: Keymap, history: History) -> Self {
        let snapshot = engine.snapshot();
        let history_sel = history.len().saturating_sub(1);
        Self {
            engine,
            keymap,
            buffer: Buffer::default(),
            history,
            focus: Focus::Input,
            history_sel,
            snapshot,
            message: None,
            copied: None,
            show_help: false,
            help_scroll: 0,
            mode: Mode::Scratch,
            edit_original: String::new(),
            discard_armed: None,
            copy_on_submit: true,
            deploy_hint: false,
            secondary: false,
            exit: None,
            effects: Vec::new(),
            hits: HitMap::default(),
        }
    }

    pub fn notify(&mut self, level: Level, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            level,
            at: Instant::now(),
        });
    }

    /// Enters `Mode::Edit` with the file's text.
    pub fn open_file(&mut self, text: &str) {
        self.mode = Mode::Edit;
        self.buffer.set_text(text);
        self.edit_original = text.to_string();
    }

    fn refresh(&mut self) {
        self.snapshot = self.engine.snapshot();
    }

    fn insert_commit(&mut self, commit: Option<String>) {
        if let Some(text) = commit {
            self.buffer.insert_str(&text);
        }
    }

    pub fn handle_key(&mut self, ev: KeyEvent) {
        if self.show_help {
            if ev.kind != KeyEventKind::Release {
                use crossterm::event::KeyCode as K;
                match ev.code {
                    K::Char('j') | K::Down => self.help_scroll = self.help_scroll.saturating_add(1),
                    K::Char('k') | K::Up => self.help_scroll = self.help_scroll.saturating_sub(1),
                    K::PageDown | K::Char(' ') => {
                        self.help_scroll = self.help_scroll.saturating_add(10)
                    }
                    K::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
                    _ => self.show_help = false,
                }
            }
            return;
        }
        let spec = KeySpec::from_event(&ev);
        if ev.kind == KeyEventKind::Release {
            // Releases only arrive under KKP; rime needs them to detect a
            // lone Shift tap. They never trigger duanyan actions.
            if self.focus == Focus::Input
                && self.engine.busy().is_none()
                && let Some(rk) = keys::to_rime(&ev)
            {
                let out = self.engine.process_key(rk);
                self.insert_commit(out.commit);
                self.refresh();
            }
            return;
        }
        // Any key other than a second cancel disarms the discard prompt.
        let armed = self.discard_armed.take();

        if let Some(action) = spec.and_then(|s| self.keymap.global.get(&s).copied()) {
            self.global_action(action, armed);
            return;
        }
        if self.engine.busy().is_some() {
            return;
        }
        match self.focus {
            Focus::History => {
                if let Some(action) = spec.and_then(|s| self.keymap.history.get(&s).copied()) {
                    self.history_action(action);
                }
            }
            Focus::Input => self.input_key(&ev, spec, armed),
        }
    }

    fn input_key(&mut self, ev: &KeyEvent, spec: Option<KeySpec>, armed: Option<Instant>) {
        if let Some(target) = spec.and_then(|s| self.keymap.compat.get(&s).copied()) {
            for k in keys::compat_sequence(target) {
                let out = self.engine.process_key(k);
                self.insert_commit(out.commit);
            }
            self.refresh();
            return;
        }
        if let Some(rk) = keys::to_rime(ev) {
            let out = self.engine.process_key(rk);
            let handled = out.handled;
            self.insert_commit(out.commit);
            self.refresh();
            if handled {
                return;
            }
        }
        if let Some(action) = spec.and_then(|s| self.keymap.input.get(&s).copied()) {
            self.input_action(action, armed);
            return;
        }
        // Printable text rime left alone, e.g. in ascii mode.
        if let crossterm::event::KeyCode::Char(c) = ev.code
            && !ev
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            self.buffer.insert_str(c.encode_utf8(&mut [0; 4]));
        }
    }

    fn global_action(&mut self, action: GlobalAction, armed: Option<Instant>) {
        match action {
            GlobalAction::Quit => match self.mode {
                Mode::Scratch => self.exit = Some(Exit::Quit),
                Mode::Stdout => self.exit = Some(Exit::Cancel),
                Mode::Edit => self.discard(armed),
            },
            GlobalAction::Help => {
                self.show_help = true;
                self.help_scroll = 0;
            }
            GlobalAction::Deploy => self.start(Maintenance::Deploy),
            GlobalAction::Sync => self.start(Maintenance::Sync),
        }
    }

    fn start(&mut self, kind: Maintenance) {
        if self.secondary {
            self.notify(Level::Warning, "已有端砚在运行，不能部署或同步");
            return;
        }
        let started = match kind {
            Maintenance::Deploy => self.engine.start_deploy(),
            Maintenance::Sync => self.engine.start_sync(),
        };
        if !started {
            self.notify(Level::Error, "无法启动 rime 维护任务");
        }
        self.refresh();
    }

    /// `Mode::Edit`: exits without saving; a modified file needs a second
    /// press.
    fn discard(&mut self, armed: Option<Instant>) {
        let confirmed = armed.is_some_and(|at| at.elapsed() <= MESSAGE_TTL);
        if confirmed || self.buffer.text() == self.edit_original {
            self.exit = Some(Exit::Cancel);
        } else {
            self.discard_armed = Some(Instant::now());
            self.notify(Level::Warning, "文件已修改，再按一次放弃修改");
        }
    }

    fn input_action(&mut self, action: InputAction, armed: Option<Instant>) {
        let b = &mut self.buffer;
        match action {
            InputAction::Submit => self.submit(),
            InputAction::Newline => b.newline(),
            InputAction::FocusHistory => {
                if !self.snapshot.composing
                    && self.mode == Mode::Scratch
                    && !self.history.is_empty()
                {
                    self.focus = Focus::History;
                    self.history_sel = self.history.len() - 1;
                }
            }
            InputAction::Cancel => match self.mode {
                Mode::Scratch => b.clear(),
                Mode::Stdout => self.exit = Some(Exit::Cancel),
                Mode::Edit => self.discard(armed),
            },
            InputAction::Left => b.left(),
            InputAction::Right => b.right(),
            InputAction::PrevLine => b.prev_line(),
            InputAction::NextLine => b.next_line(),
            InputAction::Home => b.home(),
            InputAction::End => b.end(),
            InputAction::Backspace => b.backspace(),
            InputAction::Delete => b.delete(),
            InputAction::KillWord => b.kill_word(),
            InputAction::KillToStart => b.kill_to_start(),
            InputAction::KillToEnd => b.kill_to_end(),
        }
    }

    fn submit(&mut self) {
        // Saving an empty file is legitimate, e.g. an emptied rebase todo.
        // The file is not recorded in the history: it often carries template
        // comments or a diff.
        if self.mode == Mode::Edit {
            self.effects
                .push(Effect::Save(self.buffer.text().to_string()));
            return;
        }
        if self.buffer.is_empty() {
            return;
        }
        let text = self.buffer.take();
        if let Err(e) = self
            .history
            .push(text.clone(), jiff::Timestamp::now().as_second())
        {
            self.notify(Level::Error, format!("写入历史失败：{e}"));
        }
        self.history_sel = self.history.len().saturating_sub(1);
        if self.mode == Mode::Stdout {
            self.exit = Some(Exit::Submit(text));
        } else if self.copy_on_submit {
            self.effects.push(Effect::Copy(text));
            self.copied = Some((self.history.len().saturating_sub(1), Instant::now()));
        }
    }

    fn history_action(&mut self, action: HistoryAction) {
        let n = self.history.len();
        if n == 0 {
            self.focus = Focus::Input;
            return;
        }
        match action {
            HistoryAction::Next => self.history_sel = (self.history_sel + 1).min(n - 1),
            HistoryAction::Prev => self.history_sel = self.history_sel.saturating_sub(1),
            HistoryAction::First => self.history_sel = 0,
            HistoryAction::Last => self.history_sel = n - 1,
            HistoryAction::Copy => {
                let text = self.history.entries()[self.history_sel].text.clone();
                self.effects.push(Effect::Copy(text));
                self.copied = Some((self.history_sel, Instant::now()));
            }
            HistoryAction::Recall => {
                let text = self.history.entries()[self.history_sel].text.clone();
                self.buffer.insert_str(&text);
                self.focus = Focus::Input;
            }
            HistoryAction::Delete => {
                if let Err(e) = self.history.remove(self.history_sel) {
                    self.notify(Level::Error, format!("删除历史失败：{e}"));
                }
                self.copied = None;
                if self.history.is_empty() {
                    self.focus = Focus::Input;
                    self.history_sel = 0;
                } else {
                    self.history_sel = self.history_sel.min(self.history.len() - 1);
                }
            }
            HistoryAction::FocusInput => self.focus = Focus::Input,
        }
    }

    /// Bracketed paste: bypasses rime; an active composition is discarded.
    pub fn paste(&mut self, text: &str) {
        if self.engine.busy().is_some() {
            return;
        }
        if self.snapshot.composing {
            self.engine.clear_composition();
            self.refresh();
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        self.buffer.insert_str(&text);
        self.focus = Focus::Input;
    }

    pub fn mouse(&mut self, ev: MouseEvent) {
        if self.engine.busy().is_some() || self.show_help {
            return;
        }
        let pos = Position::new(ev.column, ev.row);
        let hit = |r: &Rect| r.contains(pos);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(&(_, i)) = self.hits.candidates.iter().find(|(r, _)| hit(r)) {
                    let commit = self.engine.select_candidate(i);
                    self.insert_commit(commit);
                    self.refresh();
                } else if self.hits.prev_page.as_ref().is_some_and(hit) {
                    self.engine.change_page(true);
                    self.refresh();
                } else if self.hits.next_page.as_ref().is_some_and(hit) {
                    self.engine.change_page(false);
                    self.refresh();
                } else if let Some(&(_, i)) = self.hits.switches.iter().find(|(r, _)| hit(r)) {
                    self.engine.toggle_switch(i);
                    self.refresh();
                } else if let Some(&(_, i)) = self.hits.history_rows.iter().find(|(r, _)| hit(r)) {
                    if !self.snapshot.composing {
                        self.focus = Focus::History;
                        self.history_sel = i;
                    }
                } else if self.hits.input_area.as_ref().is_some_and(hit) {
                    self.focus = Focus::Input;
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                if self.hits.history_area.as_ref().is_some_and(hit)
                    && !self.history.is_empty()
                    && !self.snapshot.composing =>
            {
                self.focus = Focus::History;
                let n = self.history.len();
                self.history_sel = if ev.kind == MouseEventKind::ScrollUp {
                    self.history_sel.saturating_sub(1)
                } else {
                    (self.history_sel + 1).min(n - 1)
                };
            }
            _ => {}
        }
    }

    /// Periodic work: engine notifications and expiring messages.
    pub fn tick(&mut self) {
        for ev in self.engine.poll() {
            match ev {
                EngineEvent::Finished { kind, ok } => {
                    let (what, level) = match (kind, ok) {
                        (Maintenance::Deploy, true) => ("部署完成", Level::Success),
                        (Maintenance::Deploy, false) => ("部署失败，详见 rime 日志", Level::Error),
                        (Maintenance::Sync, true) => ("同步完成", Level::Success),
                        (Maintenance::Sync, false) => ("同步失败，详见 rime 日志", Level::Error),
                    };
                    if ok && kind == Maintenance::Deploy {
                        self.deploy_hint = false;
                    }
                    self.notify(level, what);
                }
                EngineEvent::SchemaChanged => {}
            }
            self.refresh();
        }
        if self
            .message
            .as_ref()
            .is_some_and(|m| m.at.elapsed() > MESSAGE_TTL)
        {
            self.message = None;
        }
        if self
            .copied
            .is_some_and(|(_, at)| at.elapsed() > MESSAGE_TTL)
        {
            self.copied = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::engine::{Candidate, KeyOutcome};
    use crossterm::event::{KeyCode, KeyEventState, ModifierKeyCode};
    use rime_dl::RimeKey;
    use rime_dl::keysym::{mask, sym};

    /// A toy engine: letters compose, space commits the input uppercased,
    /// Shift_L tap toggles ascii mode, Tab is never handled.
    #[derive(Default)]
    struct Fake {
        input: String,
        ascii: bool,
        shift_down: bool,
        received: Vec<RimeKey>,
    }

    impl ImeEngine for Fake {
        fn process_key(&mut self, key: RimeKey) -> KeyOutcome {
            self.received.push(key);
            let release = key.mask & mask::RELEASE != 0;
            if key.keycode == sym::SHIFT_L {
                if !release {
                    self.shift_down = true;
                } else if self.shift_down {
                    self.shift_down = false;
                    self.ascii = !self.ascii;
                }
                return KeyOutcome::default();
            }
            self.shift_down = false;
            if release || key.mask & mask::CONTROL != 0 {
                return KeyOutcome::default();
            }
            let c = char::from_u32(key.keycode).unwrap_or('\0');
            if !self.ascii && c.is_ascii_lowercase() {
                self.input.push(c);
                return KeyOutcome {
                    handled: true,
                    commit: None,
                };
            }
            if !self.input.is_empty() && key.keycode == sym::SPACE {
                let commit = self.input.to_uppercase();
                self.input.clear();
                return KeyOutcome {
                    handled: true,
                    commit: Some(commit),
                };
            }
            if !self.input.is_empty() && key.keycode == sym::ESCAPE {
                self.input.clear();
                return KeyOutcome {
                    handled: true,
                    commit: None,
                };
            }
            KeyOutcome::default()
        }
        fn snapshot(&self) -> ImeSnapshot {
            ImeSnapshot {
                composing: !self.input.is_empty(),
                ascii_mode: self.ascii,
                candidates: if self.input.is_empty() {
                    vec![]
                } else {
                    vec![Candidate {
                        label: "1".into(),
                        text: self.input.to_uppercase(),
                        comment: None,
                    }]
                },
                ..Default::default()
            }
        }
        fn select_candidate(&mut self, _index: usize) -> Option<String> {
            let c = self.input.to_uppercase();
            self.input.clear();
            Some(c)
        }
        fn change_page(&mut self, _backward: bool) {}
        fn toggle_switch(&mut self, _index: usize) {
            self.ascii = !self.ascii;
        }
        fn clear_composition(&mut self) {
            self.input.clear();
        }
        fn start_deploy(&mut self) -> bool {
            true
        }
        fn start_sync(&mut self) -> bool {
            true
        }
        fn busy(&self) -> Option<Maintenance> {
            None
        }
        fn poll(&mut self) -> Vec<EngineEvent> {
            vec![]
        }
    }

    fn app() -> App<Fake> {
        let cfg = Config::default_config();
        let km = Keymap::build(&cfg.keybinding, false).unwrap();
        App::new(Fake::default(), km, History::ephemeral(100))
    }

    fn key(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    fn typ(a: &mut App<Fake>, s: &str) {
        for c in s.chars() {
            a.handle_key(key(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }

    fn press(a: &mut App<Fake>, code: KeyCode) {
        a.handle_key(key(code, KeyModifiers::NONE));
    }

    #[test]
    fn compose_commit_submit() {
        let mut a = app();
        typ(&mut a, "ni");
        assert!(a.snapshot.composing);
        assert!(a.buffer.is_empty());
        typ(&mut a, " ");
        assert_eq!(a.buffer.text(), "NI");
        press(&mut a, KeyCode::Enter);
        assert!(a.buffer.is_empty());
        assert_eq!(a.history.entries()[0].text, "NI");
        assert_eq!(a.effects, vec![Effect::Copy("NI".into())]);
        assert!(a.exit.is_none());
    }

    #[test]
    fn stdout_mode_exits_with_text() {
        let mut a = app();
        a.mode = Mode::Stdout;
        typ(&mut a, "ab ");
        a.handle_key(key(KeyCode::Char('j'), KeyModifiers::CONTROL));
        typ(&mut a, "cd ");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.exit, Some(Exit::Submit("AB\nCD".into())));
        assert!(a.effects.is_empty());
        let mut a = app();
        a.mode = Mode::Stdout;
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.exit, Some(Exit::Cancel));
    }

    #[test]
    fn esc_goes_to_rime_first() {
        let mut a = app();
        a.mode = Mode::Stdout;
        typ(&mut a, "ni");
        press(&mut a, KeyCode::Esc);
        assert!(!a.snapshot.composing);
        assert!(a.exit.is_none());
    }

    #[test]
    fn tab_focus_only_when_idle() {
        let mut a = app();
        typ(&mut a, "a ");
        press(&mut a, KeyCode::Enter);
        typ(&mut a, "b");
        press(&mut a, KeyCode::Tab);
        assert_eq!(a.focus, Focus::Input, "composing blocks focus switch");
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Tab);
        assert_eq!(a.focus, Focus::History);
        // History keys never reach rime.
        let before = a.engine.received.len();
        typ(&mut a, "jk");
        assert_eq!(a.engine.received.len(), before);
        typ(&mut a, "y");
        assert_eq!(a.effects.last(), Some(&Effect::Copy("A".into())));
        press(&mut a, KeyCode::Enter); // recall
        assert_eq!(a.focus, Focus::Input);
        assert_eq!(a.buffer.text(), "A");
    }

    #[test]
    fn compat_sends_shift_tap() {
        let mut a = app();
        a.handle_key(key(KeyCode::Char('l'), KeyModifiers::ALT));
        assert!(a.snapshot.ascii_mode);
        assert_eq!(
            a.engine.received,
            vec![
                RimeKey::new(sym::SHIFT_L, 0),
                RimeKey::new(sym::SHIFT_L, mask::RELEASE)
            ]
        );
        // In ascii mode rime leaves letters to duanyan.
        typ(&mut a, "hi");
        assert_eq!(a.buffer.text(), "hi");
    }

    #[test]
    fn kkp_native_shift_tap() {
        let mut a = app();
        let ev = |kind| {
            KeyEvent::new_with_kind_and_state(
                KeyCode::Modifier(ModifierKeyCode::LeftShift),
                KeyModifiers::SHIFT,
                kind,
                KeyEventState::NONE,
            )
        };
        a.handle_key(ev(KeyEventKind::Press));
        a.handle_key(ev(KeyEventKind::Release));
        assert!(a.snapshot.ascii_mode);
    }

    #[test]
    fn readline_keys_fall_through() {
        let mut a = app();
        typ(&mut a, "ab ");
        a.handle_key(key(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(a.buffer.cursor(), 0);
        a.handle_key(key(KeyCode::Char('k'), KeyModifiers::CONTROL));
        assert!(a.buffer.is_empty());
    }

    #[test]
    fn paste_discards_composition() {
        let mut a = app();
        typ(&mut a, "ni");
        a.paste("x\r\ny");
        assert!(!a.snapshot.composing);
        assert_eq!(a.buffer.text(), "x\ny");
    }

    #[test]
    fn mouse_hits() {
        let mut a = app();
        typ(&mut a, "ni");
        a.hits.candidates.push((Rect::new(2, 10, 6, 1), 0));
        a.hits.switches.push((Rect::new(0, 20, 3, 1), 0));
        let click = |col, row| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        };
        a.mouse(click(4, 10));
        assert_eq!(a.buffer.text(), "NI");
        assert!(!a.snapshot.composing);
        a.mouse(click(1, 20));
        assert!(a.snapshot.ascii_mode);
        a.mouse(click(50, 50)); // nothing there
        assert_eq!(a.buffer.text(), "NI");
    }

    fn edit_app(text: &str) -> App<Fake> {
        let mut a = app();
        a.open_file(text);
        a
    }

    #[test]
    fn edit_mode_saves_without_history() {
        let mut a = edit_app("line\n");
        assert_eq!(a.buffer.cursor(), 0);
        typ(&mut a, "ab ");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.effects, vec![Effect::Save("ABline\n".into())]);
        assert!(a.history.is_empty());
        // The event loop sets the exit once the file is written.
        assert!(a.exit.is_none());
    }

    #[test]
    fn edit_mode_saves_empty_file() {
        let mut a = edit_app("x");
        a.handle_key(key(KeyCode::Char('k'), KeyModifiers::CONTROL));
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.effects, vec![Effect::Save(String::new())]);
    }

    #[test]
    fn edit_mode_unmodified_cancels_at_once() {
        let mut a = edit_app("x");
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.exit, Some(Exit::Cancel));
        let mut a = edit_app("x");
        a.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(a.exit, Some(Exit::Cancel));
    }

    #[test]
    fn edit_mode_modified_cancel_needs_second_press() {
        let mut a = edit_app("x");
        typ(&mut a, "ab ");
        press(&mut a, KeyCode::Esc);
        assert!(a.exit.is_none());
        assert_eq!(a.message.as_ref().map(|m| m.level), Some(Level::Warning));
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.exit, Some(Exit::Cancel));

        // Another key in between disarms the prompt; Ctrl+C counts too.
        let mut a = edit_app("x");
        typ(&mut a, "ab ");
        press(&mut a, KeyCode::Esc);
        a.handle_key(key(KeyCode::Char('a'), KeyModifiers::CONTROL));
        press(&mut a, KeyCode::Esc);
        assert!(a.exit.is_none());
        a.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(a.exit, Some(Exit::Cancel));
    }

    #[test]
    fn edit_mode_has_no_history_focus() {
        let mut a = edit_app("");
        a.history.push("old".into(), 0).unwrap();
        press(&mut a, KeyCode::Tab);
        assert_eq!(a.focus, Focus::Input);
    }

    #[test]
    fn global_quit_in_both_modes() {
        let mut a = app();
        typ(&mut a, "ni");
        a.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(a.exit, Some(Exit::Quit));
    }
}
