//! Primary-instance lock. The primary owns the user dictionary and may
//! deploy; later instances run degraded (rime cannot open the locked userdb).

use std::fs::{File, OpenOptions};
use std::path::Path;

pub enum Instance {
    /// Holds the lock until dropped.
    Primary(#[allow(dead_code)] File),
    Secondary,
}

impl Instance {
    pub fn acquire(lock_path: &Path) -> std::io::Result<Self> {
        if let Some(dir) = lock_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let f = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        match f.try_lock() {
            Ok(()) => Ok(Self::Primary(f)),
            Err(std::fs::TryLockError::WouldBlock) => Ok(Self::Secondary),
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }

    pub fn is_primary(&self) -> bool {
        matches!(self, Self::Primary(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_is_secondary() {
        let p = std::env::temp_dir().join(format!("duanyan-lock-{}", std::process::id()));
        let a = Instance::acquire(&p).unwrap();
        assert!(a.is_primary());
        let b = Instance::acquire(&p).unwrap();
        assert!(!b.is_primary());
        drop(a);
        assert!(Instance::acquire(&p).unwrap().is_primary());
        std::fs::remove_file(p).ok();
    }
}
