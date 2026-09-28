//! Copying text: OSC 52 through the terminal, or an external command.
//!
//! Native clipboard libraries are avoided on purpose: on X11 and Wayland the
//! selection belongs to the process that set it and vanishes when duanyan
//! exits, while `wl-copy`/`xclip` keep serving it from a background process.

use std::io::Write;
use std::process::{Command, Stdio};

use base64::Engine as _;

use crate::config::{ClipboardBackend, ClipboardSection};

#[derive(Debug, Clone)]
pub enum Clipboard {
    Osc52,
    Command(Vec<String>),
}

impl Clipboard {
    pub fn from_config(cfg: &ClipboardSection) -> anyhow::Result<Self> {
        Ok(match cfg.backend {
            ClipboardBackend::Osc52 => Self::Osc52,
            ClipboardBackend::Command => match &cfg.command {
                Some(c) if !c.is_empty() => Self::Command(c.clone()),
                Some(_) => anyhow::bail!("clipboard.command is empty"),
                None => Self::Command(detect_command().ok_or_else(|| {
                    anyhow::anyhow!(
                        "clipboard.backend = \"command\" but no wl-copy, xclip, xsel or pbcopy was found; set clipboard.command"
                    )
                })?),
            },
        })
    }

    /// Copies `text`. For OSC 52 the escape sequence is written to `tty`.
    pub fn copy(&self, text: &str, tty: &mut dyn Write) -> anyhow::Result<()> {
        match self {
            Self::Osc52 => {
                tty.write_all(osc52(text).as_bytes())?;
                tty.flush()?;
                Ok(())
            }
            Self::Command(argv) => {
                let mut child = Command::new(&argv[0])
                    .args(&argv[1..])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| anyhow::anyhow!("{}: {e}", argv[0]))?;
                child
                    .stdin
                    .take()
                    .expect("stdin is piped")
                    .write_all(text.as_bytes())?;
                let status = child.wait()?;
                anyhow::ensure!(status.success(), "{} exited with {status}", argv[0]);
                Ok(())
            }
        }
    }
}

pub fn osc52(text: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    format!("\x1b]52;c;{b64}\x07")
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

fn detect_command() -> Option<Vec<String>> {
    let v = |s: &[&str]| Some(s.iter().map(|x| x.to_string()).collect());
    if cfg!(target_os = "macos") && on_path("pbcopy") {
        return v(&["pbcopy"]);
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && on_path("wl-copy") {
        return v(&["wl-copy"]);
    }
    if std::env::var_os("DISPLAY").is_some() {
        if on_path("xclip") {
            return v(&["xclip", "-selection", "clipboard"]);
        }
        if on_path("xsel") {
            return v(&["xsel", "--clipboard", "--input"]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_encodes_utf8() {
        assert_eq!(osc52("你好"), "\x1b]52;c;5L2g5aW9\x07");
    }

    #[test]
    fn command_backend_pipes_text() {
        let dir = std::env::temp_dir().join(format!("duanyan-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out");
        let cb = Clipboard::Command(vec![
            "sh".into(),
            "-c".into(),
            format!("cat > {}", out.display()),
        ]);
        cb.copy("多\n行", &mut std::io::sink()).unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "多\n行");
        std::fs::remove_dir_all(dir).ok();
    }
}
