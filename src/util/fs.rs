use std::fs;
use std::path::Path;

use crate::error::Result;

pub fn atomic_rename(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to)?;
    if let Some(parent) = to.parent() {
        fsync_dir(parent)?;
    }
    Ok(())
}

pub fn fsync_dir(dir: &Path) -> Result<()> {
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

pub fn ensure_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_rename_works() {
        let dir = tempdir().unwrap();
        let from = dir.path().join("old");
        let to = dir.path().join("new");
        fs::write(&from, "data").unwrap();
        atomic_rename(&from, &to).unwrap();
        assert!(to.exists());
        assert!(!from.exists());
    }
}
