use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use anyhow::{Result, anyhow, Context};
use futures::future::join_all;
use rand::Rng;
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, ETAG, IF_NONE_MATCH, RANGE};
use reqwest::Client;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use crate::core::constants;

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

        if let Some(cl) = content_length {
            if accept_ranges && cl > constants::CHUNKED_DOWNLOAD_THRESHOLD {
                self.download_chunked(url, destination, cl).await?;
            } else {
                self.download_streaming(url, destination).await?;
            }
        } else {
            self.download_streaming(url, destination).await?;
        }

        Ok(new_etag)
    }

    async fn download_streaming(&self, url: &str, destination: &Path) -> Result<()> {
        let resp = self.client.get(url).send().await?;
        let mut file = tokio::fs::File::create(destination).await?;
        let mut stream = resp.bytes_stream();
        use futures::TryStreamExt;
        while let Some(chunk) = stream.try_next().await? {
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        Ok(())
    }

    async fn download_chunked(&self, url: &str, destination: &Path, total_size: u64) -> Result<()> {
        let chunk_size = (total_size / self.max_concurrent_chunks).max(constants::MIN_CHUNK_SIZE);
        let file = Arc::new(Mutex::new(
            OpenOptions::new().create(true).write(true).truncate(true).open(destination)
                .with_context(|| format!("Failed to create {:?}", destination))?
        ));
        file.lock().await.set_len(total_size)?;

        let mut tasks = Vec::new();
        for i in 0..self.max_concurrent_chunks {
            let start = i * chunk_size;
            let end = if i == self.max_concurrent_chunks - 1 {
                total_size - 1
            } else {
                (start + chunk_size - 1).min(total_size - 1)
            };
            if start > end { break; }
            let client = self.client.clone();
            let file_clone = Arc::clone(&file);
            let url = url.to_string();

            tasks.push(tokio::spawn(async move {
                let resp = client
                    .get(&url)
                    .header(RANGE, format!("bytes={}-{}", start, end))
                    .send()
                    .await?;
                let bytes = resp.bytes().await?;
                let mut f = file_clone.lock().await;
                f.seek(SeekFrom::Start(start))?;
                f.write_all(&bytes)?;
                Ok::<(), anyhow::Error>(())
            }));
        }

        for res in join_all(tasks).await {
            res??;
        }
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
}
