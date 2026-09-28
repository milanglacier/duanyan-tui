//! Locating and loading the librime shared library at runtime.

use std::ffi::CStr;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::ffi;

/// Oldest librime whose `RimeTraits` has `log_dir`, needed to keep glog off
/// stderr while the TUI owns the terminal.
pub const MIN_VERSION: (u32, u32, u32) = (1, 8, 0);

/// A loaded librime and its API function table.
pub struct Library {
    // Keeps the shared object mapped for as long as `api` is used.
    _lib: libloading::Library,
    api: *const ffi::RimeApi,
    path: PathBuf,
}

// SAFETY: `api` points at a static table inside the loaded library and is
// only read. Thread-safety of individual calls is librime's contract.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}

impl Library {
    /// Loads librime from `path`, which is either a filesystem path or a bare
    /// library name resolved by the system loader.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        // SAFETY: loading librime runs its static initializers, which only
        // register rime modules.
        let lib =
            unsafe { libloading::Library::new(path.as_os_str()) }.map_err(|e| Error::Load {
                path: path.to_owned(),
                reason: e.to_string(),
            })?;
        // SAFETY: `rime_get_api` has this signature in every librime >= 1.0.
        let api = unsafe {
            let get_api: libloading::Symbol<ffi::RimeGetApiFn> =
                lib.get(b"rime_get_api\0").map_err(|e| Error::Load {
                    path: path.to_owned(),
                    reason: e.to_string(),
                })?;
            get_api()
        };
        if api.is_null() {
            return Err(Error::Load {
                path: path.to_owned(),
                reason: "rime_get_api returned NULL".into(),
            });
        }
        let lib = Self {
            _lib: lib,
            api,
            path: path.to_owned(),
        };
        let version = lib.version().unwrap_or_default();
        match parse_version(&version) {
            Some(v) if v >= MIN_VERSION => Ok(lib),
            _ => Err(Error::Version {
                found: if version.is_empty() {
                    "unknown".into()
                } else {
                    version
                },
                required: format!("{}.{}.{}", MIN_VERSION.0, MIN_VERSION.1, MIN_VERSION.2),
            }),
        }
    }

    /// Tries each candidate in order and returns the first that loads,
    /// together with every failure for diagnostics.
    pub fn open_first<I, P>(candidates: I) -> Result<Self, Vec<Error>>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut errors = Vec::new();
        for candidate in candidates {
            match Self::open(candidate) {
                Ok(lib) => return Ok(lib),
                Err(e) => errors.push(e),
            }
        }
        Err(errors)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn api(&self) -> &ffi::RimeApi {
        // SAFETY: non-null, checked in `open`, and valid while `_lib` lives.
        unsafe { &*self.api }
    }

    pub fn version(&self) -> Option<String> {
        let f = crate::api_fn!(self.api(), get_version)?;
        // SAFETY: returns a static C string or NULL.
        let p = unsafe { f() };
        if p.is_null() {
            return None;
        }
        Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }
}

/// Platform-specific file name the system loader should resolve.
pub const fn default_library_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "librime.1.dylib"
    } else {
        "librime.so.1"
    }
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.split('.').map(|p| {
        p.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse::<u32>()
            .ok()
    });
    Some((
        it.next()??,
        it.next().flatten().unwrap_or(0),
        it.next().flatten().unwrap_or(0),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("1.17.0"), Some((1, 17, 0)));
        assert_eq!(parse_version("1.8"), Some((1, 8, 0)));
        assert_eq!(parse_version("1.9.0-rc1"), Some((1, 9, 0)));
        assert_eq!(parse_version(""), None);
        assert!(parse_version("1.7.3").unwrap() < MIN_VERSION);
    }
}
