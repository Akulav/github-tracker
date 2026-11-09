//! HTTP helpers: a shared client, text/JSON GETs with retries and streamed file downloads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

pub const USER_AGENT: &str = concat!("AkulavTracker/", env!("CARGO_PKG_VERSION"));

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .expect("HTTP client")
}

pub async fn get_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let mut last_err = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
        }
        match client.get(url).send().await.and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => last_err = Some(e),
            },
            Err(e) => {
                // 4xx won't get better by retrying.
                if e.status().is_some_and(|s| s.is_client_error()) {
                    return Err(e).with_context(|| format!("GET {url}"));
                }
                last_err = Some(e)
            }
        }
    }
    Err(last_err.unwrap()).with_context(|| format!("GET {url}"))
}

pub async fn get_text(client: &reqwest::Client, url: &str) -> Result<String> {
    let bytes = get_bytes(client, url).await?;
    Ok(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())
}

pub async fn get_json<T: serde::de::DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    let text = get_text(client, url).await?;
    serde_json::from_str(&text).with_context(|| format!("invalid JSON from {url}"))
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

/// Stream `url` into `dest` (via `dest.part`). `on_bytes(done)` reports progress.
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    on_bytes: &(dyn Fn(u64) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = part_path(dest);
    let result: Result<()> = async {
        let resp = client.get(url).send().await?.error_for_status()?;
        let mut file = tokio::fs::File::create(&part).await?;
        let mut stream = resp.bytes_stream();
        let mut done = 0u64;
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            on_bytes(done);
        }
        file.flush().await?;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e).with_context(|| format!("downloading {url}"));
    }
    let _ = tokio::fs::remove_file(dest).await;
    tokio::fs::rename(&part, dest).await.with_context(|| format!("could not write {}", dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_paths() {
        assert_eq!(part_path(Path::new("/a/b.exe")), PathBuf::from("/a/b.exe.part"));
    }
}
