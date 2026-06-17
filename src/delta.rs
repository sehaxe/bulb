use std::path::{Path, PathBuf};

use crate::error::{BulbError, Result};

pub struct DeltaResult {
    pub old_file: PathBuf,
    pub new_file: PathBuf,
    pub delta_file: PathBuf,
    pub ratio: f64,
}

pub struct DeltaEngine {
    temp_dir: PathBuf,
}

impl DeltaEngine {
    pub fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?.keep();
        Ok(Self { temp_dir })
    }

    pub fn create_delta(&self, old_path: &Path, new_path: &Path) -> Result<DeltaResult> {
        let old_size = std::fs::metadata(old_path)?.len();

        let delta_name = format!(
            "{}.delta",
            new_path.file_name().and_then(|n| n.to_str()).unwrap_or("pkg")
        );
        let delta_path = self.temp_dir.join(&delta_name);

        let old_bytes = std::fs::read(old_path)?;
        let new_bytes = std::fs::read(new_path)?;

        let mut delta = Vec::new();
        bsdiff::diff(&old_bytes, &new_bytes, &mut delta)
            .map_err(|e| BulbError::Delta(format!("bsdiff failed: {e}")))?;

        std::fs::write(&delta_path, &delta)?;

        let ratio = if old_size > 0 {
            delta.len() as f64 / old_size as f64
        } else {
            1.0
        };

        Ok(DeltaResult {
            old_file: old_path.to_path_buf(),
            new_file: new_path.to_path_buf(),
            delta_file: delta_path,
            ratio,
        })
    }

    pub fn apply_delta(&self, old_path: &Path, delta_path: &Path, new_path: &Path) -> Result<()> {
        let old_bytes = std::fs::read(old_path)?;
        let delta_bytes = std::fs::read(delta_path)?;

        let mut new_bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&delta_bytes);
        bsdiff::patch(&old_bytes, &mut cursor, &mut new_bytes)
            .map_err(|e| BulbError::Delta(format!("bspatch failed: {e}")))?;

        if let Some(parent) = new_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(new_path, &new_bytes)?;

        Ok(())
    }

    pub fn should_use_delta(old_size: u64, delta_size: u64) -> bool {
        const MIN_DELTA_RATIO: f64 = 0.3;
        const MIN_FILE_SIZE: u64 = 1024 * 1024;

        if old_size < MIN_FILE_SIZE {
            return false;
        }

        let ratio = delta_size as f64 / old_size as f64;
        ratio < MIN_DELTA_RATIO
    }

    pub fn cleanup(self) {
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}

impl Default for DeltaEngine {
    fn default() -> Self {
        Self::new().expect("failed to create delta temp dir")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_create_and_apply() {
        let engine = DeltaEngine::new().unwrap();
        let dir = &engine.temp_dir;

        let old_path = dir.join("old.bin");
        let new_path = dir.join("new.bin");
        let result_path = dir.join("result.bin");

        let old_data: Vec<u8> = (0..2_000_000).map(|i| ((i * 7 + 13) % 256) as u8).collect();
        let mut new_data = old_data.clone();
        for i in 100_000..100_200 {
            new_data[i] = 42;
        }
        new_data.extend_from_slice(&vec![99u8; 1_000_000]);

        std::fs::write(&old_path, &old_data).unwrap();
        std::fs::write(&new_path, &new_data).unwrap();

        let delta = engine.create_delta(&old_path, &new_path).unwrap();
        assert!(delta.delta_file.exists());

        engine.apply_delta(&old_path, &delta.delta_file, &result_path).unwrap();
        assert_eq!(std::fs::read(&result_path).unwrap(), new_data);

        engine.cleanup();
    }

    #[test]
    fn should_use_delta_threshold() {
        assert!(!DeltaEngine::should_use_delta(500, 100));
        assert!(!DeltaEngine::should_use_delta(1024 * 1024, 1024 * 1024));
        assert!(DeltaEngine::should_use_delta(10 * 1024 * 1024, 1024 * 1024));
    }
}
