use std::path::PathBuf;
use std::sync::Arc;

use reqwest::Client;
use tokio::sync::Semaphore;

use crate::error::{BulbError, Result};

pub struct DownloadClient {
    client: Client,
    sem: Arc<Semaphore>,
    cache_dir: PathBuf,
}

impl DownloadClient {
    pub fn new(cache_dir: PathBuf, max_concurrent: usize) -> Result<Self> {
        let client = Client::builder()
            .user_agent("bulb/0.1")
            .gzip(true)
            .brotli(true)
            .zstd(true)
            .build()
            .map_err(|e| BulbError::Config(format!("http client: {e}")))?;
        Ok(Self {
            client,
            sem: Arc::new(Semaphore::new(max_concurrent)),
            cache_dir,
        })
    }

    pub async fn download(&self, url: &str, expected_hash: Option<&str>) -> Result<PathBuf> {
        let filename = url.rsplit('/').next().unwrap_or("package.pkg.tar.zst");
        let dest = self.cache_dir.join(filename);

        if dest.exists() {
            return Ok(dest);
        }

        let _permit = self.sem.clone().acquire_owned().await
            .map_err(|e| BulbError::Config(format!("semaphore closed: {e}")))?;

        let response = self.client.get(url).send().await
            .map_err(|e| BulbError::Config(format!("download failed: {e}")))?;

        if !response.status().is_success() {
            return Err(BulbError::Config(format!(
                "download failed: HTTP {} from {}",
                response.status(),
                url
            )));
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let bytes = response.bytes().await
            .map_err(|e| BulbError::Config(format!("download read failed: {e}")))?;

        if let Some(expected) = expected_hash {
            let actual = blake3::hash(&bytes).to_hex().to_string();
            if actual != expected {
                return Err(BulbError::Config(format!(
                    "hash mismatch: expected {expected}, got {actual}"
                )));
            }
        }

        std::fs::write(&dest, &bytes)?;

        Ok(dest)
    }

    pub async fn download_all(&self, items: Vec<DownloadItem>) -> Result<Vec<PathBuf>> {
        let mut handles = Vec::new();
        let client = Arc::new(self.clone());

        for item in items {
            let client = client.clone();
            handles.push(tokio::spawn(async move {
                client.download(&item.url, item.hash.as_deref()).await
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await
                .map_err(|e| BulbError::Config(format!("task failed: {e}")))??);
        }
        Ok(results)
    }
}

impl Clone for DownloadClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            sem: self.sem.clone(),
            cache_dir: self.cache_dir.clone(),
        }
    }
}

pub struct DownloadItem {
    pub url: String,
    pub hash: Option<String>,
    pub filename: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_download_client() {
        let cache = tempdir().unwrap();
        let client = DownloadClient::new(cache.path().to_path_buf(), 4).unwrap();
        assert!(client.cache_dir.exists());
    }
}
