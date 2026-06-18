use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::stream::StreamExt;
use reqwest::Client;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

use crate::error::{BulbError, Result};

const MAX_RETRIES: u32 = 5;
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const READ_TIMEOUT: Duration = Duration::from_secs(60);

pub struct DownloadClient {
    client: Client,
    sem: Arc<Semaphore>,
    cache_dir: PathBuf,
}

impl DownloadClient {
    pub fn new(cache_dir: PathBuf, max_concurrent: usize) -> Result<Self> {
        let client = Client::builder()
            .user_agent("bulb/0.2")
            .gzip(true)
            .brotli(true)
            .zstd(true)
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
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
        let part_path = dest.with_extension("pkg.tar.zst.part");

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut last_error = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let backoff = INITIAL_BACKOFF * 2u32.pow(attempt - 1);
                eprintln!("retry {} in {:?}...", attempt, backoff);
                tokio::time::sleep(backoff).await;
            }

            match self.try_download(url, &dest, &part_path, expected_hash).await {
                Ok(path) => return Ok(path),
                Err(e) => {
                    eprintln!("attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| BulbError::Config("download failed after all retries".into())))
    }

    async fn try_download(
        &self,
        url: &str,
        dest: &PathBuf,
        part_path: &PathBuf,
        expected_hash: Option<&str>,
    ) -> Result<PathBuf> {
        let _permit = self.sem.clone().acquire_owned().await
            .map_err(|e| BulbError::Config(format!("semaphore closed: {e}")))?;

        let downloaded_bytes = if part_path.exists() {
            std::fs::metadata(part_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        let mut request = self.client.get(url);

        if downloaded_bytes > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded_bytes));
            eprintln!("resuming from {} bytes", downloaded_bytes);
        }

        let response = request.send().await
            .map_err(|e| BulbError::Config(format!("download failed: {e}")))?;

        let status = response.status();

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(BulbError::Config(format!("not found: {}", url)));
        }

        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            std::fs::remove_file(part_path).ok();
            return self.try_download_full(url, dest, part_path, expected_hash).await;
        }

        if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(BulbError::Config(format!(
                "download failed: HTTP {} from {}",
                status, url
            )));
        }

        let content_length = response.content_length().unwrap_or(0);
        let total_size = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            downloaded_bytes + content_length
        } else {
            content_length
        };

        if total_size > 0 {
            eprintln!("downloading {} bytes...", total_size);
        }

        let mut file = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(part_path)
                .await
                .map_err(|e| BulbError::Config(format!("open part file: {e}")))?
        } else {
            tokio::fs::File::create(part_path)
                .await
                .map_err(|e| BulbError::Config(format!("create part file: {e}")))?
        };

        let mut stream = response.bytes_stream();
        let mut downloaded = downloaded_bytes;
        let mut last_progress = Instant::now();
        let start_time = Instant::now();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| BulbError::Config(format!("read chunk: {e}")))?;
            file.write_all(&chunk).await
                .map_err(|e| BulbError::Config(format!("write chunk: {e}")))?;
            downloaded += chunk.len() as u64;

            if last_progress.elapsed() >= Duration::from_secs(1) || downloaded == total_size {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    (downloaded as f64) / elapsed / 1024.0 / 1024.0
                } else {
                    0.0
                };

                if total_size > 0 {
                    let progress = (downloaded as f64 / total_size as f64 * 100.0) as u32;
                    eprint!("\r  {}%  {:.1} MB/s  ", progress, speed);
                } else {
                    eprint!("\r  {:.1} MB  {:.1} MB/s  ", downloaded as f64 / 1024.0 / 1024.0, speed);
                }

                last_progress = Instant::now();
            }
        }

        if total_size > 0 {
            eprintln!();
        }

        file.flush().await
            .map_err(|e| BulbError::Config(format!("flush: {e}")))?;
        drop(file);

        std::fs::rename(part_path, dest)
            .map_err(|e| BulbError::Config(format!("rename: {e}")))?;

        if let Some(expected) = expected_hash {
            let bytes = std::fs::read(dest)?;
            let actual = blake3::hash(&bytes).to_hex().to_string();
            if actual != expected {
                std::fs::remove_file(dest).ok();
                return Err(BulbError::Config(format!(
                    "hash mismatch: expected {expected}, got {actual}"
                )));
            }
        }

        Ok(dest.clone())
    }

    async fn try_download_full(
        &self,
        url: &str,
        dest: &PathBuf,
        part_path: &PathBuf,
        expected_hash: Option<&str>,
    ) -> Result<PathBuf> {
        std::fs::remove_file(part_path).ok();

        let _permit = self.sem.clone().acquire_owned().await
            .map_err(|e| BulbError::Config(format!("semaphore closed: {e}")))?;

        let response = self.client.get(url).send().await
            .map_err(|e| BulbError::Config(format!("download failed: {e}")))?;

        if !response.status().is_success() {
            return Err(BulbError::Config(format!(
                "download failed: HTTP {} from {}",
                response.status(), url
            )));
        }

        let bytes = response.bytes().await
            .map_err(|e| BulbError::Config(format!("read failed: {e}")))?;

        if let Some(expected) = expected_hash {
            let actual = blake3::hash(&bytes).to_hex().to_string();
            if actual != expected {
                return Err(BulbError::Config(format!(
                    "hash mismatch: expected {expected}, got {actual}"
                )));
            }
        }

        std::fs::write(dest, &bytes)?;

        Ok(dest.clone())
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

    #[test]
    fn retry_constants() {
        assert_eq!(MAX_RETRIES, 5);
        assert_eq!(INITIAL_BACKOFF, Duration::from_secs(1));
    }
}
