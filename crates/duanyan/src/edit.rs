//! Editing a file as `$EDITOR`: load it into the buffer, write it back.

use std::path::PathBuf;

use anyhow::Context as _;

/// Larger files are refused: the whole buffer is laid out on every frame.
const MAX_SIZE: u64 = 1 << 20;

const BOM: &str = "\u{feff}";

pub struct EditFile {
    pub path: PathBuf,
    /// Text as loaded into the buffer (line endings normalized, BOM removed).
    pub original: String,
    crlf: bool,
    bom: bool,
}

impl EditFile {
    /// A missing file loads as empty and is created on save.
    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let bytes = match std::fs::metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
            Ok(m) if m.is_dir() => anyhow::bail!("{} is a directory", path.display()),
            Ok(m) if m.len() > MAX_SIZE => {
                anyhow::bail!("{} is larger than 1 MiB", path.display())
            }
            Ok(_) => std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
        };
        let (original, crlf, bom) =
            decode(bytes).with_context(|| format!("{} is not valid UTF-8", path.display()))?;
        Ok(Self {
            path,
            original,
            crlf,
            bom,
        })
    }

    /// Restores CRLF line endings and the BOM, then overwrites the file in
    /// place, which keeps its inode, mode and owner.
    pub fn save(&self, text: &str) -> std::io::Result<()> {
        std::fs::write(&self.path, encode(text, self.crlf, self.bom))
    }
}

/// Returns (text, crlf, bom). Line endings are normalized only when every
/// line break is CRLF; mixed files are kept as they are.
fn decode(mut bytes: Vec<u8>) -> Result<(String, bool, bool), std::string::FromUtf8Error> {
    let bom = bytes.starts_with(BOM.as_bytes());
    if bom {
        bytes.drain(..BOM.len());
    }
    let text = String::from_utf8(bytes)?;
    let lf = text.matches('\n').count();
    let crlf = lf > 0 && text.matches("\r\n").count() == lf;
    let text = if crlf {
        text.replace("\r\n", "\n")
    } else {
        text
    };
    Ok((text, crlf, bom))
}

fn encode(text: &str, crlf: bool, bom: bool) -> Vec<u8> {
    let mut out = String::with_capacity(text.len() + 3);
    if bom {
        out.push_str(BOM);
    }
    if crlf {
        out.push_str(&text.replace('\n', "\r\n"));
    } else {
        out.push_str(text);
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(raw: &str, text: &str, crlf: bool, bom: bool) {
        let decoded = decode(raw.as_bytes().to_vec()).unwrap();
        assert_eq!(decoded, (text.to_string(), crlf, bom), "decode {raw:?}");
        assert_eq!(encode(text, crlf, bom), raw.as_bytes(), "encode {raw:?}");
    }

    #[test]
    fn formats_round_trip() {
        round_trip("", "", false, false);
        round_trip("a\nb\n", "a\nb\n", false, false);
        round_trip("a\nb", "a\nb", false, false);
        round_trip("a\r\nb\r\n", "a\nb\n", true, false);
        round_trip("a\r\nb\n", "a\r\nb\n", false, false);
        round_trip("\u{feff}中文\r\n", "中文\n", true, true);
        round_trip("a\r\r\n", "a\r\n", true, false);
    }

    #[test]
    fn edits_keep_crlf() {
        assert_eq!(encode("x\ny", true, false), b"x\r\ny");
    }

    #[test]
    fn rejects_invalid_utf8() {
        assert!(decode(vec![b'a', 0xff]).is_err());
    }

    #[test]
    fn load_and_save() {
        let dir = std::env::temp_dir().join(format!("duanyan-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("new.txt");
        let _ = std::fs::remove_file(&path);
        let f = EditFile::load(path.clone()).unwrap();
        assert_eq!(f.original, "");
        f.save("你好\n").unwrap();
        let f = EditFile::load(path.clone()).unwrap();
        assert_eq!(f.original, "你好\n");
        assert!(EditFile::load(dir.clone()).is_err());
        std::fs::write(&path, [0xffu8]).unwrap();
        assert!(EditFile::load(path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
