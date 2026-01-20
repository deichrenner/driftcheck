use crate::config::Config;
use crate::error::{DriftcheckError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use tracing::debug;

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    queries: Vec<String>,
    created_at: DateTime<Utc>,
}

pub struct CacheStats {
    pub entries: usize,
    pub size_bytes: u64,
    pub path: PathBuf,
}

/// Get the cache directory path
fn get_cache_dir() -> Result<PathBuf> {
    let git_root = Config::find_git_root()?;
    let config = Config::load().unwrap_or_default();
    Ok(git_root.join(&config.cache.dir))
}

/// Generate a cache key from diff content
fn cache_key(diff: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(diff.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8]) // Use first 8 bytes for shorter filenames
}

// We need hex encoding
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

/// Get cached search queries for a diff
pub fn get_queries(diff: &str) -> Option<Vec<String>> {
    let cache_dir = get_cache_dir().ok()?;
    let key = cache_key(diff);
    let cache_file = cache_dir.join(format!("{}.json", key));

    if !cache_file.exists() {
        return None;
    }

    let content = fs::read_to_string(&cache_file).ok()?;
    let entry: CacheEntry = serde_json::from_str(&content).ok()?;

    // Check TTL
    let config = Config::load().unwrap_or_default();
    let ttl = chrono::Duration::seconds(config.cache.ttl as i64);
    let age = Utc::now() - entry.created_at;

    if age > ttl {
        debug!("Cache entry expired");
        let _ = fs::remove_file(&cache_file);
        return None;
    }

    Some(entry.queries)
}

/// Store search queries in cache
pub fn store_queries(diff: &str, queries: &[String]) -> Result<()> {
    let cache_dir = get_cache_dir()?;

    // Create cache directory if it doesn't exist
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)
            .map_err(|e| DriftcheckError::CacheError(e.to_string()))?;
    }

    let key = cache_key(diff);
    let cache_file = cache_dir.join(format!("{}.json", key));

    let entry = CacheEntry {
        queries: queries.to_vec(),
        created_at: Utc::now(),
    };

    let content = serde_json::to_string_pretty(&entry)
        .map_err(|e| DriftcheckError::CacheError(e.to_string()))?;

    fs::write(&cache_file, content)
        .map_err(|e| DriftcheckError::CacheError(e.to_string()))?;

    debug!("Cached queries to {}", cache_file.display());

    Ok(())
}

/// Clear the cache
pub fn clear() -> Result<()> {
    let cache_dir = get_cache_dir()?;

    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)
            .map_err(|e| DriftcheckError::CacheError(e.to_string()))?;
    }

    Ok(())
}

/// Get cache statistics
pub fn stats() -> Result<CacheStats> {
    let cache_dir = get_cache_dir()?;

    if !cache_dir.exists() {
        return Ok(CacheStats {
            entries: 0,
            size_bytes: 0,
            path: cache_dir,
        });
    }

    let mut entries = 0;
    let mut size_bytes = 0;

    for entry in fs::read_dir(&cache_dir)
        .map_err(|e| DriftcheckError::CacheError(e.to_string()))?
        .flatten()
    {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                entries += 1;
                size_bytes += meta.len();
            }
        }
    }

    Ok(CacheStats {
        entries,
        size_bytes,
        path: cache_dir,
    })
}
