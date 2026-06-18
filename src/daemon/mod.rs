pub mod cache;
pub mod handlers;
pub mod ipc;

use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::signal;
use tokio::sync::Notify;

use crate::error::{BulbError, Result};

#[derive(Clone)]
pub struct DaemonConfig {
    pub socket_path: PathBuf,
    pub pid_path: PathBuf,
    pub db_path: PathBuf,
    pub store_path: PathBuf,
    pub cache_path: PathBuf,
    pub max_cache_size: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/bulbd.sock"),
            pid_path: PathBuf::from("/run/bulbd.pid"),
            db_path: PathBuf::from("/var/lib/bulb/bulb.db"),
            store_path: PathBuf::from("/var/lib/bulb/content"),
            cache_path: PathBuf::from("/var/lib/bulb/cache"),
            max_cache_size: 2 * 1024 * 1024 * 1024,
        }
    }
}

pub async fn run_daemon(config: DaemonConfig) -> Result<()> {
    if config.socket_path.exists() {
        std::fs::remove_file(&config.socket_path)?;
    }

    if let Some(parent) = config.socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&config.pid_path, std::process::id().to_string())?;

    let listener = UnixListener::bind(&config.socket_path)
        .map_err(|e| BulbError::Config(format!("failed to bind socket: {e}")))?;

    let shutdown = Arc::new(Notify::new());
    let shutdown_clone = shutdown.clone();

    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        shutdown_clone.notify_one();
    });

    let db = Arc::new(tokio::sync::Mutex::new(
        crate::db::Database::open(&config.db_path)?,
    ));

    let cache = Arc::new(cache::CacheManager::new(
        config.cache_path.clone(),
        config.max_cache_size,
    ));

    eprintln!("bulbd listening on {}", config.socket_path.display());

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _)) => {
                        let db = db.clone();
                        let cache = cache.clone();
                        let config = config.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, db, cache, config).await {
                                eprintln!("connection error: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("accept error: {e}");
                    }
                }
            }
            _ = shutdown.notified() => {
                eprintln!("shutting down...");
                break;
            }
        }
    }

    let _ = std::fs::remove_file(&config.socket_path);
    let _ = std::fs::remove_file(&config.pid_path);

    Ok(())
}

async fn handle_connection(
    mut stream: tokio::net::UnixStream,
    db: Arc<tokio::sync::Mutex<crate::db::Database>>,
    cache: Arc<cache::CacheManager>,
    config: DaemonConfig,
) -> Result<()> {
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let n = stream.read(&mut buf).await
            .map_err(|e| BulbError::Config(format!("read error: {e}")))?;

        if n == 0 {
            break;
        }

        let request: ipc::JsonRpcRequest = serde_json::from_slice(&buf[..n])
            .map_err(|e| BulbError::Config(format!("invalid JSON-RPC: {e}")))?;

        let response = handlers::handle_request(request, &db, &cache, &config).await;

        let response_bytes = serde_json::to_vec(&response)
            .map_err(|e| BulbError::Config(format!("serialize response: {e}")))?;

        tokio::io::AsyncWriteExt::write_all(&mut stream, &response_bytes).await
            .map_err(|e| BulbError::Config(format!("write error: {e}")))?;
        tokio::io::AsyncWriteExt::write_all(&mut stream, b"\n").await
            .map_err(|e| BulbError::Config(format!("write error: {e}")))?;
    }

    Ok(())
}
