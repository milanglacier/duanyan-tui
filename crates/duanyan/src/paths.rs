//! XDG directories and discovery of librime and the shared rime data dir.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn home_dir() -> PathBuf {
    env::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    match env::var_os(var) {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => home_dir().join(fallback),
    }
}

/// `$XDG_CONFIG_HOME/duanyan`, also on macOS.
pub fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("duanyan")
}

/// `$XDG_STATE_HOME/duanyan`: history, rime logs and the instance lock.
pub fn state_dir() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state").join("duanyan")
}

pub fn default_config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn default_user_data_dir() -> PathBuf {
    config_dir().join("rime")
}

/// Expands a leading `~/` in paths taken from the config file.
pub fn expand_tilde(p: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => home_dir().join(rest),
        Err(_) => p.to_owned(),
    }
}

/// Environment lookups, injectable for tests.
pub trait Env {
    fn var(&self, key: &str) -> Option<OsString>;
    fn exists(&self, p: &Path) -> bool;
}

pub struct RealEnv;

impl Env for RealEnv {
    fn var(&self, key: &str) -> Option<OsString> {
        env::var_os(key).filter(|v| !v.is_empty())
    }
    fn exists(&self, p: &Path) -> bool {
        p.exists()
    }
}

/// Where librime may live, in the order they are tried. Bare names are
/// resolved by the system loader.
pub fn librime_candidates(configured: Option<&Path>, env: &dyn Env) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = configured {
        out.push(expand_tilde(p));
    }
    if let Some(p) = env.var("DUANYAN_LIBRIME_PATH") {
        out.push(PathBuf::from(p));
    }
    let name = rime_dl::default_library_name();
    out.push(PathBuf::from(name));
    let home = env.var("HOME").map(PathBuf::from).unwrap_or_else(home_dir);
    let user = env.var("USER").map(|u| u.to_string_lossy().into_owned());
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        out.push(PathBuf::from(
            "/Library/Input Methods/Squirrel.app/Contents/Frameworks/librime.1.dylib",
        ));
        dirs.extend(["/opt/homebrew/lib", "/usr/local/lib"].map(PathBuf::from));
        dirs.push(home.join(".nix-profile/lib"));
    } else {
        dirs.push(PathBuf::from("/run/current-system/sw/lib"));
        if let Some(u) = &user {
            dirs.push(PathBuf::from(format!("/etc/profiles/per-user/{u}/lib")));
        }
        dirs.push(home.join(".nix-profile/lib"));
        dirs.extend(
            [
                "/usr/lib",
                "/usr/lib64",
                "/usr/lib/x86_64-linux-gnu",
                "/usr/lib/aarch64-linux-gnu",
                "/usr/local/lib",
            ]
            .map(PathBuf::from),
        );
    }
    for d in dirs {
        let p = d.join(name);
        if env.exists(&p) {
            out.push(p);
        }
    }
    out
}

/// Resolves `shared_data_dir`. `None` means nothing was found; librime then
/// uses the user data dir for everything, which is a normal setup.
pub fn find_shared_data_dir(configured: Option<&Path>, env: &dyn Env) -> Option<PathBuf> {
    if let Some(p) = configured {
        return Some(expand_tilde(p));
    }
    if let Some(p) = env.var("DUANYAN_RIME_SHARED_DIR") {
        return Some(PathBuf::from(p));
    }
    let data_dirs = env
        .var("XDG_DATA_DIRS")
        .map(|v| env::split_paths(&v).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    let mut candidates: Vec<PathBuf> = data_dirs.iter().map(|d| d.join("rime-data")).collect();
    candidates.extend(["/usr/share/rime-data", "/usr/local/share/rime-data"].map(PathBuf::from));
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(
            "/Library/Input Methods/Squirrel.app/Contents/SharedSupport",
        ));
        candidates.push(PathBuf::from("/opt/homebrew/share/rime-data"));
    }
    candidates.into_iter().find(|p| env.exists(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct FakeEnv {
        vars: HashMap<&'static str, &'static str>,
        files: HashSet<PathBuf>,
    }

    impl Env for FakeEnv {
        fn var(&self, key: &str) -> Option<OsString> {
            self.vars.get(key).map(OsString::from)
        }
        fn exists(&self, p: &Path) -> bool {
            self.files.contains(p)
        }
    }

    #[test]
    fn librime_order() {
        let mut env = FakeEnv::default();
        env.vars
            .insert("DUANYAN_LIBRIME_PATH", "/nix/store/x/lib/librime.so.1");
        env.vars.insert("HOME", "/home/u");
        env.vars.insert("USER", "u");
        env.files.insert(PathBuf::from("/usr/lib/librime.so.1"));
        let c = librime_candidates(Some(Path::new("/opt/rime.so")), &env);
        if !cfg!(target_os = "macos") {
            assert_eq!(
                c,
                vec![
                    PathBuf::from("/opt/rime.so"),
                    PathBuf::from("/nix/store/x/lib/librime.so.1"),
                    PathBuf::from("librime.so.1"),
                    PathBuf::from("/usr/lib/librime.so.1"),
                ]
            );
        }
    }

    #[test]
    fn shared_dir_order() {
        let mut env = FakeEnv::default();
        env.vars
            .insert("XDG_DATA_DIRS", "/a/share:/run/current-system/sw/share");
        env.files
            .insert(PathBuf::from("/run/current-system/sw/share/rime-data"));
        env.files.insert(PathBuf::from("/usr/share/rime-data"));
        assert_eq!(
            find_shared_data_dir(None, &env),
            Some(PathBuf::from("/run/current-system/sw/share/rime-data"))
        );
        env.vars.insert("DUANYAN_RIME_SHARED_DIR", "/env/rime-data");
        assert_eq!(
            find_shared_data_dir(None, &env),
            Some(PathBuf::from("/env/rime-data"))
        );
        assert_eq!(
            find_shared_data_dir(Some(Path::new("/cfg")), &env),
            Some(PathBuf::from("/cfg"))
        );
        assert_eq!(find_shared_data_dir(None, &FakeEnv::default()), None);
    }
}
