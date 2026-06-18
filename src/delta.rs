use std::path::{Path, PathBuf};

use crate::error::{BulbError, Result};

pub struct DeltaEngine {
    temp_dir: PathBuf,
}

impl DeltaEngine {
    pub fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?.keep();
        Ok(Self { temp_dir })
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
    fn apply_delta_roundtrip() {
        use std::io::Write;

        let engine = DeltaEngine::new().unwrap();
        let dir = &engine.temp_dir;

        let old_path = dir.join("old.bin");
        let new_path = dir.join("new.bin");
        let delta_path = dir.join("old.delta");
        let result_path = dir.join("result.bin");

        let old_data: Vec<u8> = (0..2_000_000).map(|i| ((i * 7 + 13) % 256) as u8).collect();
        std::fs::write(&old_path, &old_data).unwrap();

        let mut new_data = old_data.clone();
        for i in 100_000..100_200 {
            new_data[i] = 42;
        }
        new_data.extend_from_slice(&vec![99u8; 1_000_000]);
        std::fs::write(&new_path, &new_data).unwrap();

        let mut delta = Vec::new();
        bsdiff::diff(&old_data, &new_data, &mut delta).unwrap();
        let mut f = std::fs::File::create(&delta_path).unwrap();
        f.write_all(&delta).unwrap();

        engine.apply_delta(&old_path, &delta_path, &result_path).unwrap();
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
