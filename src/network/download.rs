use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use anyhow::{Result, anyhow, Context};
use futures::future::join_all;
use rand::Rng;
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, ETAG, IF_NONE_MATCH, IF_RANGE, RANGE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use crate::core::constants;

/// Persisted progress of a chunked transfer: which byte-ranges have already
/// been downloaded in full. Kept as a sibling of the `.part` file so an
/// interrupted (>5 MiB, multi-connection) download can continue from the
/// last completed chunk instead of restarting the whole file from zero.
#[derive(Serialize, Deserialize)]
struct ChunkBitmap {
    total_size: u64,
    chunk_size: u64,
    completed: BTreeSet<u64>,
}

impl ChunkBitmap {
    fn new(total_size: u64, chunk_size: u64) -> Self {
        Self {
            total_size,
            chunk_size,
            completed: BTreeSet::new(),
        }
    }
}

#[derive(Clone)]
pub struct Downloader {
    client: Client,
    max_concurrent_chunks: u64,
    max_concurrent_packages: usize,
    max_retries: u32,
    base_delay_ms: u64,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    pub fn new() -> Self {
        let client = Client::builder()
            .pool_idle_timeout(Duration::from_secs(constants::POOL_IDLE_TIMEOUT_SECS))
            .tcp_keepalive(Duration::from_secs(constants::TCP_KEEPALIVE_SECS))
            .pool_max_idle_per_host(constants::POOL_MAX_IDLE_PER_HOST)
            // Bound connection establishment and per-read stalls so an
            // unresponsive mirror cannot hang the client indefinitely.
            .connect_timeout(Duration::from_secs(constants::DOWNLOAD_CONNECT_TIMEOUT_SECS))
            .read_timeout(Duration::from_secs(constants::DOWNLOAD_READ_TIMEOUT_SECS))
            .build()
            .expect("build reqwest client");
        Self {
            client,
            max_concurrent_chunks: constants::DEFAULT_MAX_CONCURRENT_CHUNKS as u64,
            max_concurrent_packages: constants::DEFAULT_MAX_CONCURRENT_PACKAGES,
            max_retries: constants::DEFAULT_MAX_RETRIES,
            base_delay_ms: constants::DEFAULT_BASE_DELAY_MS,
        }
    }

    pub async fn package(&self, url: &str, destination: &Path) -> Result<Option<String>> {
        self.download_with_retry(url, destination, None).await
    }

    pub async fn conditional(&self, url: &str, destination: &Path, etag: Option<&str>) -> Result<Option<String>> {
        self.download_with_retry(url, destination, etag).await
    }

    async fn download_with_retry(&self, url: &str, destination: &Path, etag: Option<&str>) -> Result<Option<String>> {
        let mut last_err = None;
        for attempt in 0..=self.max_retries {
            match self.try_download(url, destination, etag).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_err = Some(e);
                    if attempt < self.max_retries {
                        let delay = self.backoff(attempt);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("Download failed after {} retries", self.max_retries)))
    }

    fn backoff(&self, attempt: u32) -> Duration {
        let base = self.base_delay_ms * 2u64.pow(attempt);
        let jitter = rand::rng().random_range(0..=base / 2);
        Duration::from_millis(base + jitter)
    }

    async fn try_download(&self, url: &str, destination: &Path, etag: Option<&str>) -> Result<Option<String>> {
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut req = self.client.head(url);
        if let Some(tag) = etag {
            req = req.header(IF_NONE_MATCH, tag);
        }
        let head_resp = req.send().await?;
        let status = head_resp.status();

        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(Some(etag.unwrap_or_default().to_string()));
        }
        if !status.is_success() {
            return Err(anyhow!("HTTP {}", status));
        }

        let new_etag = head_resp.headers().get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string());

        let accept_ranges = head_resp.headers().get(ACCEPT_RANGES)
            .map(|v| v.to_str().unwrap_or("")) == Some("bytes");
        let content_length = head_resp.headers().get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().unwrap_or("").parse::<u64>().ok());

        // Always download into a sibling `.part` file and only promote it to the
        // final destination once the full body has been received, so an
        // interrupted download never leaves a truncated file at `destination`.
        let part_path = part_path_for(destination);

        if let Some(cl) = content_length {
            if accept_ranges && cl > constants::CHUNKED_DOWNLOAD_THRESHOLD {
                self.download_chunked(url, &part_path, cl).await?;
            } else {
                let part_size = self.partial_size(&part_path, accept_ranges).await;
                // A complete sibling `.part` is promoted directly; integrity is
                // still enforced later via the archive checksum.
                if let Some(cl) = content_length
                    && accept_ranges
                    && part_size == cl
                {
                    tokio::fs::rename(&part_path, destination).await?;
                    return Ok(new_etag);
                }
                // Never resume from a partial that is larger than the remote
                // object — it belongs to a stale, different transfer.
                let resume_from = match content_length {
                    Some(cl) if part_size < cl => part_size,
                    _ => 0,
                };
                // If-Range makes the server reject the range request when the
                // resource changed since the ETag we saw; we then restart cleanly.
                self.download_streaming(url, &part_path, resume_from, new_etag.as_deref()).await?;
            }
        } else {
            // Without a known length we cannot resume reliably.
            self.download_streaming(url, &part_path, 0, None).await?;
        }

        tokio::fs::rename(&part_path, destination).await?;
        Ok(new_etag)
    }

    async fn partial_size(&self, part: &Path, accept_ranges: bool) -> u64 {
        if !accept_ranges {
            return 0;
        }
        match tokio::fs::metadata(part).await {
            Ok(meta) => meta.len(),
            Err(_) => 0,
        }
    }

    async fn download_streaming(&self, url: &str, part: &Path, mut resume_from: u64, etag: Option<&str>) -> Result<()> {
        loop {
            if resume_from == 0 {
                let resp = self.client.get(url).send().await?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(anyhow!("HTTP {}", status));
                }
                let mut file = tokio::fs::File::create(part).await?;
                write_stream(&mut file, resp).await?;
                return Ok(());
            }

            let mut req = self.client
                .get(url)
                .header(RANGE, format!("bytes={}-", resume_from));
            if let Some(tag) = etag {
                req = req.header(IF_RANGE, tag);
            }
            let resp = req.send().await?;

            match resp.status() {
                reqwest::StatusCode::PARTIAL_CONTENT => {
                    let mut file = tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(part)
                        .await?;
                    write_stream(&mut file, resp).await?;
                    return Ok(());
                }
                reqwest::StatusCode::RANGE_NOT_SATISFIABLE => {
                    // Server no longer has the byte range (stale partial file);
                    // restart the transfer from scratch.
                    resume_from = 0;
                }
                status if status.is_success() => {
                    // Server ignored the Range header; re-download completely.
                    let mut file = tokio::fs::File::create(part).await?;
                    write_stream(&mut file, resp).await?;
                    return Ok(());
                }
                status => return Err(anyhow!("HTTP {}", status)),
            }
        }
    }

    async fn download_chunked(&self, url: &str, part: &Path, total_size: u64) -> Result<()> {
        let chunk_size = (total_size / self.max_concurrent_chunks).max(constants::MIN_CHUNK_SIZE);
        let ranges = chunk_ranges(total_size, chunk_size);
        let bm_path = bitmap_path(part);

        // A sidecar bitmap is only trusted when it matches the remote object
        // AND the part file is already the full size — otherwise completed
        // entries could alias bytes of a stale, different transfer, so every
        // chunk is treated as incomplete and refetched.
        let part_len = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
        let bitmap = match load_bitmap(&bm_path).await {
            Some(bm) if bm.total_size == total_size
                && bm.chunk_size == chunk_size
                && part_len == total_size => bm,
            _ => ChunkBitmap::new(total_size, chunk_size),
        };

        let file = Arc::new(Mutex::new(
            OpenOptions::new()
                .create(true)
                // Never truncate: chunk bytes already downloaded by a
                // previous, interrupted run are preserved for resume.
                .truncate(false)
                .write(true)
                .open(part)
                .with_context(|| format!("Failed to create {:?}", part))?
        ));
        file.lock().await.set_len(total_size)?;

        let bitmap = Arc::new(Mutex::new(bitmap));
        let mut tasks = Vec::new();
        for (i, (start, end)) in ranges.iter().enumerate() {
            let idx = i as u64;
            let start = *start;
            let end = *end;
            if bitmap.lock().await.completed.contains(&idx) {
                continue;
            }
            let client = self.client.clone();
            let file_clone = Arc::clone(&file);
            let bitmap_clone = Arc::clone(&bitmap);
            let part_path = part.to_path_buf();
            let url = url.to_string();

            tasks.push(tokio::spawn(async move {
                let resp = client
                    .get(&url)
                    .header(RANGE, format!("bytes={}-{}", start, end))
                    .send()
                    .await?;
                let bytes = resp.bytes().await?;
                {
                    let mut f = file_clone.lock().await;
                    f.seek(SeekFrom::Start(start))?;
                    f.write_all(&bytes)?;
                }
                // Persist this chunk as completed only after its bytes are
                // fully written, so the resume marker never outlives the data
                // it stands for.
                let mut bm = bitmap_clone.lock().await;
                bm.completed.insert(idx);
                save_bitmap(&part_path, &bm).await?;
                Ok::<(), anyhow::Error>(())
            }));
        }

        for res in join_all(tasks).await {
            res??;
        }

        // On success the sidecar is dropped; the caller promotes the `.part`
        // file to its final destination.
        let _ = tokio::fs::remove_file(&bm_path).await;
        Ok(())
    }

    pub async fn download_many(&self, urls: &[(String, PathBuf)]) -> Vec<(usize, Result<Option<String>>)> {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.max_concurrent_packages));
        let mut tasks = Vec::with_capacity(urls.len());

        for (idx, (url, dest)) in urls.iter().enumerate() {
            let url = url.clone();
            let dest = dest.clone();
            let dl = self.clone();
            let sem = Arc::clone(&semaphore);

            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("acquire semaphore");
                let result = dl.package(&url, &dest).await;
                (idx, result)
            }));
        }

        let mut results = Vec::with_capacity(tasks.len());
        for task in join_all(tasks).await {
            results.push(task.expect("join download task"));
        }
        results
    }

    pub fn get_client(&self) -> &Client {
        &self.client
    }

    pub async fn check_endpoint_availability(&self, url: &str) -> bool {
        self.client.head(url).send().await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

}

fn part_path_for(destination: &Path) -> PathBuf {
    let mut os_string = destination.as_os_str().to_os_string();
    os_string.push(".part");
    PathBuf::from(os_string)
}

/// Sidecar holding download progress: `<file>.xcs.part.bitmap`.
fn bitmap_path(part: &Path) -> PathBuf {
    let mut os_string = part.as_os_str().to_os_string();
    os_string.push(".bitmap");
    PathBuf::from(os_string)
}

/// Half-open byte ranges `[start, end)` covering a file of `total_size`
/// bytes in fixed `chunk_size` pieces.
fn chunk_ranges(total_size: u64, chunk_size: u64) -> Vec<(u64, u64)> {
    let mut ranges = Vec::new();
    let mut start = 0u64;
    while start < total_size {
        let end = (start + chunk_size).min(total_size);
        ranges.push((start, end));
        start = end;
    }
    ranges
}

async fn load_bitmap(bm_path: &Path) -> Option<ChunkBitmap> {
    let bytes = tokio::fs::read(bm_path).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

async fn save_bitmap(part: &Path, bitmap: &ChunkBitmap) -> Result<()> {
    let bm_path = bitmap_path(part);
    let mut os_string = bm_path.as_os_str().to_os_string();
    os_string.push(".tmp");
    let tmp = PathBuf::from(os_string);
    let bytes = serde_json::to_vec(bitmap)?;
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, &bm_path).await?;
    Ok(())
}

async fn write_stream(file: &mut tokio::fs::File, resp: reqwest::Response) -> Result<()> {
    use futures::TryStreamExt;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.try_next().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_exponential_growth() {
        let d = Downloader::new();
        let prev = d.backoff(0).as_millis();
        for attempt in 1..4 {
            let cur = d.backoff(attempt).as_millis();
            assert!(cur > prev, "attempt {} should be larger than {}", attempt, attempt - 1);
        }
    }

    #[test]
    fn test_backoff_with_jitter_within_range() {
        let d = Downloader::new();
        for attempt in 0..4 {
            let dur = d.backoff(attempt);
            let ms = dur.as_millis() as u64;
            let base = d.base_delay_ms * 2u64.pow(attempt);
            assert!(ms >= base, "attempt {}: {} < {}", attempt, ms, base);
            assert!(ms <= base + base / 2, "attempt {}: {} > {}", attempt, ms, base + base / 2);
        }
    }

    #[tokio::test]
    async fn test_downloader_construction() {
        let d = Downloader::new();
        assert_eq!(d.max_retries, 3);
        assert_eq!(d.max_concurrent_packages, 8);
    }

    #[test]
    fn test_chunk_ranges_partition() {
        let ranges = chunk_ranges(10, 4);
        assert_eq!(ranges, vec![(0, 4), (4, 8), (8, 10)]);
        assert_eq!(ranges.iter().map(|(_, e)| *e).max(), Some(10));
        // Exact multiple: no partial tail chunk.
        assert_eq!(chunk_ranges(8, 4), vec![(0, 4), (4, 8)]);
        // Empty file yields no ranges.
        assert!(chunk_ranges(0, 4).is_empty());
    }

    #[test]
    fn test_chunk_ranges_covers_full_file_without_overlap() {
        let total = 25_000_000u64;
        let chunk = 1_048_576u64;
        let ranges = chunk_ranges(total, chunk);
        let mut cursor = 0u64;
        for (start, end) in &ranges {
            assert_eq!(cursor, *start, "ranges must be contiguous");
            assert!(end > start);
            cursor = *end;
        }
        assert_eq!(cursor, total);
    }

    #[tokio::test]
    async fn test_chunk_bitmap_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let part = dir.path().join("pkg-1.0.xcs.part");
        let mut bitmap = ChunkBitmap::new(100, 25);
        bitmap.completed.insert(0);
        bitmap.completed.insert(3);
        save_bitmap(&part, &bitmap).await.expect("save bitmap");

        let loaded = load_bitmap(&bitmap_path(&part)).await.expect("load bitmap");
        assert_eq!(loaded.total_size, 100);
        assert_eq!(loaded.completed, BTreeSet::from([0, 3]));
        // No temporary litter after the atomic rename.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bitmap.tmp"))
            .collect();
        assert!(leftovers.is_empty(), "leftover tmp files: {leftovers:?}");
    }
}
