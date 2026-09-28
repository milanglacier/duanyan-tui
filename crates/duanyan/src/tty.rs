//! Terminal setup on `/dev/tty`, so stdout stays free for `--stdout`.
//!
//! Capability queries (kitty keyboard protocol, background color) are done
//! by hand: crossterm's own KKP query writes to stdout.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{ExecutableCommand, QueueableCommand, cursor};
use ratatui::layout::Rect;

pub fn open_tty() -> std::io::Result<File> {
    OpenOptions::new().read(true).write(true).open("/dev/tty")
}

#[derive(Debug, Default, Clone)]
pub struct Probe {
    pub kkp: bool,
    /// Raw OSC 11 reply, if any.
    pub background: Option<String>,
}

/// Queries KKP support and the background color, ending with a DA1 request
/// that every terminal answers so the wait stays short. Raw mode must be on.
pub fn probe(tty: &mut File, timeout: Duration) -> Probe {
    let mut probe = Probe::default();
    query(tty, b"\x1b]11;?\x1b\\\x1b[?u\x1b[c", timeout, |buf| {
        parse_replies(buf, &mut probe)
    });
    probe
}

/// Writes `request` and reads replies until `done` returns true or the
/// timeout passes.
fn query(tty: &mut File, request: &[u8], timeout: Duration, mut done: impl FnMut(&[u8]) -> bool) {
    if tty.write_all(request).is_err() || tty.flush().is_err() {
        return;
    }
    let deadline = Instant::now() + timeout;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 256];
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let wait = deadline - now;
        let ts = rustix::event::Timespec {
            tv_sec: wait.as_secs() as _,
            tv_nsec: wait.subsec_nanos() as _,
        };
        let mut fds = [rustix::event::PollFd::new(
            &*tty,
            rustix::event::PollFlags::IN,
        )];
        match rustix::event::poll(&mut fds, Some(&ts)) {
            Ok(n) if n > 0 => {}
            _ => break,
        }
        match tty.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
        if done(&buf) {
            break;
        }
    }
}

/// The cursor row via DSR (`CSI 6 n`) on the tty. crossterm's own
/// `cursor::position` writes the request to stdout, which `--stdout` owns.
pub fn cursor_row(tty: &mut File) -> Option<u16> {
    let mut row = None;
    query(tty, b"\x1b[6n", Duration::from_millis(500), |buf| {
        row = parse_cursor_report(buf);
        row.is_some()
    });
    row
}

fn parse_cursor_report(buf: &[u8]) -> Option<u16> {
    let s = std::str::from_utf8(buf).ok()?;
    let start = s.rfind("\x1b[")?;
    let body = s[start + 2..].strip_suffix('R')?;
    let (row, _col) = body.split_once(';')?;
    row.parse::<u16>().ok()?.checked_sub(1)
}

/// Reserves `height` rows below the cursor for the inline UI, scrolling the
/// screen up when too close to the bottom.
pub fn reserve_inline(tty: &mut File, height: u16) -> std::io::Result<Rect> {
    let (cols, rows) = crossterm::terminal::size()?;
    let height = height.min(rows);
    let mut row = cursor_row(tty).unwrap_or(rows.saturating_sub(height));
    if row + height > rows {
        let scroll = row + height - rows;
        tty.write_all("\n".repeat(scroll as usize).as_bytes())?;
        tty.flush()?;
        row = rows - height;
    }
    Ok(Rect::new(0, row, cols, height))
}

/// Blanks the inline area and leaves the cursor at its top-left.
pub fn clear_inline(tty: &mut File, area: Rect) -> std::io::Result<()> {
    for y in area.top()..area.bottom() {
        tty.queue(cursor::MoveTo(0, y))?
            .queue(Clear(ClearType::CurrentLine))?;
    }
    tty.queue(cursor::MoveTo(0, area.y))?;
    tty.flush()
}

/// Sends fd 2 to a file while the TUI owns the terminal; librime's glog
/// and anything else writing to stderr would corrupt the screen. Restored
/// on drop.
pub struct StderrRedirect(rustix::fd::OwnedFd);

impl StderrRedirect {
    pub fn to_file(path: &std::path::Path) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let saved = rustix::io::dup(rustix::stdio::stderr())?;
        rustix::stdio::dup2_stderr(&file)?;
        Ok(Self(saved))
    }
}

impl Drop for StderrRedirect {
    fn drop(&mut self) {
        let _ = rustix::stdio::dup2_stderr(&self.0);
    }
}

/// Updates `probe` from the bytes read so far; true once DA1 has arrived.
fn parse_replies(buf: &[u8], probe: &mut Probe) -> bool {
    let s = String::from_utf8_lossy(buf);
    if let Some(start) = s.find("\x1b]11;") {
        let rest = &s[start..];
        if let Some(end) = rest
            .find(['\x07'])
            .or_else(|| rest.find("\x1b\\").map(|i| i + 1))
        {
            probe.background = Some(rest[..=end].to_owned());
        }
    }
    // CSI ? <flags> u
    let mut rest = &*s;
    while let Some(i) = rest.find("\x1b[?") {
        let tail = &rest[i + 3..];
        let digits = tail.chars().take_while(char::is_ascii_digit).count();
        if tail[digits..].starts_with('u') {
            probe.kkp = true;
        }
        if tail[digits..].starts_with(';') || tail[digits..].starts_with('c') {
            // DA1: CSI ? Ps ; ... c
            if let Some(end) = tail.find('c')
                && tail[..end].chars().all(|c| c.is_ascii_digit() || c == ';')
            {
                return true;
            }
        }
        rest = tail;
    }
    false
}

#[derive(Debug, Clone, Copy)]
pub struct Modes {
    pub alternate_screen: bool,
    pub mouse: bool,
    pub kkp: bool,
}

const KKP_FLAGS: KeyboardEnhancementFlags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
    .union(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
    .union(KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES)
    .union(KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS);

pub fn enter(tty: &mut File, m: Modes) -> std::io::Result<()> {
    if m.alternate_screen {
        tty.execute(EnterAlternateScreen)?;
    }
    tty.execute(EnableBracketedPaste)?;
    if m.mouse {
        tty.execute(EnableMouseCapture)?;
    }
    if m.kkp {
        tty.execute(PushKeyboardEnhancementFlags(KKP_FLAGS))?;
    }
    Ok(())
}

/// Undoes [`enter`] and raw mode. Errors are ignored: this also runs from
/// the panic hook.
pub fn restore(m: Modes) {
    let Ok(mut file) = open_tty() else {
        let _ = crossterm::terminal::disable_raw_mode();
        return;
    };
    let tty = &mut file;
    if m.kkp {
        let _ = tty.execute(PopKeyboardEnhancementFlags);
    }
    if m.mouse {
        let _ = tty.execute(DisableMouseCapture);
    }
    let _ = tty.execute(DisableBracketedPaste);
    if m.alternate_screen {
        let _ = tty.execute(LeaveAlternateScreen);
    }
    let _ = tty.execute(cursor::Show);
    let _ = crossterm::terminal::disable_raw_mode();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies() {
        let mut p = Probe::default();
        assert!(!parse_replies(
            b"\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\\x1b[?1u",
            &mut p
        ));
        assert!(p.kkp);
        assert_eq!(
            p.background.as_deref(),
            Some("\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\")
        );
        assert!(parse_replies(
            b"\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\\x1b[?1u\x1b[?62;22c",
            &mut p
        ));
        let mut p = Probe::default();
        assert!(parse_replies(b"\x1b[?65;4;1c", &mut p));
        assert_eq!(parse_cursor_report(b"\x1b[12;1R"), Some(11));
        assert_eq!(parse_cursor_report(b"\x1b[12;1"), None);
        assert!(!p.kkp);
        assert!(p.background.is_none());
    }
}
