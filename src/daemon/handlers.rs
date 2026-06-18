use std::sync::Arc;

use tokio::sync::Mutex;

use crate::db::Database;
use crate::daemon::cache::CacheManager;
use crate::daemon::ipc::{JsonRpcRequest, JsonRpcResponse};
use crate::daemon::DaemonConfig;

pub async fn handle_request(
    request: JsonRpcRequest,
    db: &Arc<Mutex<Database>>,
    cache: &Arc<CacheManager>,
    config: &DaemonConfig,
) -> JsonRpcResponse {
    let id = request.id.clone();

    match request.method.as_str() {
        "status" => handle_status(db, cache, config, id).await,
        "list-cache" => handle_list_cache(cache, id).await,
        "clean-cache" => handle_clean_cache(cache, id).await,
        "verify" => handle_verify(request.params, id).await,
        "download" => handle_download(request.params, cache, id).await,
        _ => JsonRpcResponse::error(id, -32601, format!("method not found: {}", request.method)),
    }
}

async fn handle_status(
    db: &Arc<Mutex<Database>>,
    cache: &Arc<CacheManager>,
    _config: &DaemonConfig,
    id: Option<serde_json::Value>,
) -> JsonRpcResponse {
    let db = db.lock().await;
    let current_gen = db.current_generation().ok().flatten();
    let cache_size = cache.total_size().unwrap_or(0);
    let cache_entries = cache.list().map(|e| e.len()).unwrap_or(0);

    let result = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "current_generation": current_gen,
        "cache_size_bytes": cache_size,
        "cache_entries": cache_entries,
        "pid": std::process::id(),
    });

    JsonRpcResponse::success(id, result)
}

async fn handle_list_cache(
    cache: &Arc<CacheManager>,
    id: Option<serde_json::Value>,
) -> JsonRpcResponse {
    match cache.list() {
        Ok(entries) => {
            let items: Vec<serde_json::Value> = entries.iter().map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "size": e.size,
                    "path": e.path,
                })
            }).collect();

            JsonRpcResponse::success(id, serde_json::json!({
                "entries": items,
                "total_size": entries.iter().map(|e| e.size).sum::<u64>(),
            }))
        }
        Err(e) => JsonRpcResponse::error(id, -32000, format!("cache error: {e}")),
    }
}

async fn handle_clean_cache(
    cache: &Arc<CacheManager>,
    id: Option<serde_json::Value>,
) -> JsonRpcResponse {
    match cache.clean() {
        Ok(freed) => JsonRpcResponse::success(id, serde_json::json!({
            "freed_bytes": freed,
        })),
        Err(e) => JsonRpcResponse::error(id, -32000, format!("clean error: {e}")),
    }
}

async fn handle_verify(
    params: serde_json::Value,
    id: Option<serde_json::Value>,
) -> JsonRpcResponse {
    let file_path = params.get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcResponse::error(id.clone(), -32602, "missing 'path' parameter".into()));

    match file_path {
        Ok(path) => {
            match std::fs::read(path) {
                Ok(data) => {
                    let hash = blake3::hash(&data);
                    JsonRpcResponse::success(id, serde_json::json!({
                        "hash": hash.to_hex().to_string(),
                        "size": data.len(),
                    }))
                }
                Err(e) => JsonRpcResponse::error(id, -32000, format!("read error: {e}")),
            }
        }
        Err(resp) => resp,
    }
}

async fn handle_download(
    params: serde_json::Value,
    cache: &Arc<CacheManager>,
    id: Option<serde_json::Value>,
) -> JsonRpcResponse {
    let url = params.get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcResponse::error(id.clone(), -32602, "missing 'url' parameter".into()));

    match url {
        Ok(url) => {
            let filename = url.rsplit('/').next().unwrap_or("package.pkg.tar.zst");
            let dest = cache.get(filename).unwrap_or_else(|| {
                std::path::PathBuf::from(format!("{}/{}", cache.cache_dir.display(), filename))
            });

            match reqwest::get(url).await {
                Ok(resp) => {
                    match resp.bytes().await {
                        Ok(bytes) => {
                            if let Err(e) = std::fs::write(&dest, &bytes) {
                                return JsonRpcResponse::error(id, -32000, format!("write error: {e}"));
                            }
                            JsonRpcResponse::success(id, serde_json::json!({
                                "path": dest,
                                "size": bytes.len(),
                            }))
                        }
                        Err(e) => JsonRpcResponse::error(id, -32000, format!("download error: {e}")),
                    }
                }
                Err(e) => JsonRpcResponse::error(id, -32000, format!("request error: {e}")),
            }
        }
        Err(resp) => resp,
    }
}
