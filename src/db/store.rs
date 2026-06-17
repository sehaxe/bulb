use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{BulbError, Result};

pub struct ContentStore {
    root: PathBuf,
}

impl ContentStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    pub fn add(&self, data: &[u8]) -> Result<String> {
        let hash = blake3::hash(data);
        let hex = hash.to_hex().to_string();
        let dest = self.object_path(&hex);

        if !dest.exists() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, data)?;
        }

        Ok(hex)
    }

    pub fn add_file(&self, path: &Path) -> Result<String> {
        let data = fs::read(path)?;
        self.add(&data)
    }

    pub fn link(&self, hash: &str, dest: &Path) -> Result<()> {
        let src = self.object_path(hash);
        if !src.exists() {
            return Err(BulbError::StoreObjectMissing(hash.to_string()));
        }
        if dest.exists() || dest.symlink_metadata().is_ok() {
            fs::remove_file(dest)?;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::hard_link(&src, dest)?;
        Ok(())
    }

    pub fn unlink(path: &Path) -> Result<()> {
        if path.is_dir() {
            let _ = fs::remove_dir(path);
        } else if path.exists() || path.symlink_metadata().is_ok() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn exists(&self, hash: &str) -> bool {
        self.object_path(hash).exists()
    }

    pub fn object_path(&self, hash: &str) -> PathBuf {
        let (prefix, rest) = hash.split_at(2.min(hash.len()));
        self.root.join(prefix).join(rest)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn add_and_link() {
        let store_dir = tempdir().unwrap();
        let content_dir = tempdir().unwrap();
        let store = ContentStore::new(store_dir.path().to_path_buf());
        store.init().unwrap();

        let data = b"hello world content";
        let hash = store.add(data).unwrap();

        assert!(store.exists(&hash));

        let dest = content_dir.path().join("usr/bin/test");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        store.link(&hash, &dest).unwrap();

        assert!(dest.exists());
        assert_eq!(fs::read(&dest).unwrap(), data);
    }

    #[test]
    fn deduplicates_identical_content() {
        let store_dir = tempdir().unwrap();
        let store = ContentStore::new(store_dir.path().to_path_buf());
        store.init().unwrap();

        let data = b"identical content for dedup test";
        let hash1 = store.add(data).unwrap();
        let hash2 = store.add(data).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn unlink_removes_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "test").unwrap();

        ContentStore::unlink(&file).unwrap();
        assert!(!file.exists());
    }
}
