use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::Result;

pub struct CacheManager {
    pub cache_dir: PathBuf,
    max_size: u64,
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub last_access: SystemTime,
}

impl CacheManager {
    pub fn new(cache_dir: PathBuf, max_size: u64) -> Self {
        let _ = std::fs::create_dir_all(&cache_dir);
        Self { cache_dir, max_size }
    }

    pub fn list(&self) -> Result<Vec<CacheEntry>> {
        let mut entries = Vec::new();

        if !self.cache_dir.exists() {
            return Ok(entries);
        }

        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = std::fs::metadata(&path)?;
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();

            if name.ends_with(".pkg.tar.zst") || name.ends_with(".pkg.tar.bz3") {
                let last_access = metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH);
                entries.push(CacheEntry {
                    name,
                    path,
                    size: metadata.len(),
                    last_access,
                });
            }
        }

        entries.sort_by(|a, b| b.last_access.cmp(&a.last_access));
        Ok(entries)
    }

    pub fn total_size(&self) -> Result<u64> {
        let entries = self.list()?;
        Ok(entries.iter().map(|e| e.size).sum())
    }

    pub fn evict(&self) -> Result<u64> {
        let mut entries = self.list()?;
        let total: u64 = entries.iter().map(|e| e.size).sum();

        if total <= self.max_size {
            return Ok(0);
        }

        let mut freed = 0u64;
        entries.sort_by(|a, b| a.last_access.cmp(&b.last_access));

        for entry in &entries {
            if total - freed <= self.max_size {
                break;
            }
            let size = entry.size;
            if std::fs::remove_file(&entry.path).is_ok() {
                freed += size;
            }
        }

        Ok(freed)
    }

    pub fn clean(&self) -> Result<u64> {
        self.evict()
    }

    pub fn get(&self, name: &str) -> Option<PathBuf> {
        let path = self.cache_dir.join(name);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}
