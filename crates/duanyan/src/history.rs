//! Submitted texts, persisted as JSON Lines.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix seconds.
    pub ts: i64,
    pub text: String,
}

#[derive(Debug)]
pub struct History {
    entries: Vec<Entry>,
    path: Option<PathBuf>,
    max_entries: usize,
}

impl History {
    /// In-memory only.
    pub fn ephemeral(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            path: None,
            max_entries,
        }
    }

    /// Loads the last `max_entries` entries from `path`. Malformed lines are
    /// skipped so a partially written file never blocks startup.
    pub fn load(path: PathBuf, max_entries: usize) -> std::io::Result<Self> {
        let mut entries = Vec::new();
        let mut lines = 0;
        match File::open(&path) {
            Ok(f) => {
                for line in BufReader::new(f).lines() {
                    let line = line?;
                    lines += 1;
                    if let Ok(e) = serde_json::from_str::<Entry>(&line) {
                        entries.push(e);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let excess = entries.len().saturating_sub(max_entries);
        entries.drain(..excess);
        let h = Self {
            entries,
            path: Some(path),
            max_entries,
        };
        if lines > 2 * max_entries.max(1) {
            h.rewrite()?;
        }
        Ok(h)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Appends with a single `write` in append mode, so concurrent instances
    /// do not interleave lines.
    pub fn push(&mut self, text: String, ts: i64) -> std::io::Result<()> {
        let entry = Entry { ts, text };
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
            line.push('\n');
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?
                .write_all(line.as_bytes())?;
        }
        self.entries.push(entry);
        let excess = self.entries.len().saturating_sub(self.max_entries);
        self.entries.drain(..excess);
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> std::io::Result<()> {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.rewrite()?;
        }
        Ok(())
    }

    /// Rewrites the file from memory via a temporary file and rename.
    fn rewrite(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let tmp = tmp_path(path);
        {
            let mut f = File::create(&tmp)?;
            for e in &self.entries {
                let line = serde_json::to_string(e).map_err(std::io::Error::other)?;
                writeln!(f, "{line}")?;
            }
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("duanyan-hist-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn persist_trim_and_delete() {
        let dir = tmpdir("a");
        let path = dir.join("history.jsonl");
        let mut h = History::load(path.clone(), 3).unwrap();
        for (i, t) in ["一", "二\n行", "三", "四"].iter().enumerate() {
            h.push(t.to_string(), i as i64).unwrap();
        }
        assert_eq!(h.len(), 3);
        let h2 = History::load(path.clone(), 3).unwrap();
        assert_eq!(
            h2.entries()
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            ["二\n行", "三", "四"]
        );
        let mut h2 = h2;
        h2.remove(0).unwrap();
        let h3 = History::load(path, 3).unwrap();
        assert_eq!(h3.len(), 2);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn compacts_and_skips_garbage() {
        let dir = tmpdir("b");
        let path = dir.join("history.jsonl");
        let mut text = String::from("not json\n");
        for i in 0..10 {
            text.push_str(&format!("{{\"ts\":{i},\"text\":\"t{i}\"}}\n"));
        }
        std::fs::write(&path, text).unwrap();
        let h = History::load(path.clone(), 2).unwrap();
        assert_eq!(h.entries()[1].text, "t9");
        let lines = std::fs::read_to_string(&path).unwrap().lines().count();
        assert_eq!(lines, 2);
        std::fs::remove_dir_all(dir).ok();
    }
}
