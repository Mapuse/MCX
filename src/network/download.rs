use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use futures::future::join_all;
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, RANGE};
use reqwest::Client;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Downloader {
    client: Client,
    max_concurrent_chunks: u64,
}

impl Downloader {
    pub fn new() -> Self {
        let client = Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(15))
            .tcp_keepalive(std::time::Duration::from_secs(15))
            .build()
            .unwrap();
        Self { client, max_concurrent_chunks: 8 }
    }

    pub async fn download_package(&self, url: &str, destination: &Path) -> Result<()> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let head_resp = self.client.head(url).send().await?;
        if !head_resp.status().is_success() {
            return Err(anyhow!("HTTP Status {}", head_resp.status()));
        }

        let accept_ranges = head_resp.headers().get(ACCEPT_RANGES).map(|v| v.to_str().unwrap_or("")) == Some("bytes");
        let content_length = head_resp.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().unwrap_or("").parse::<u64>().ok());

        if accept_ranges && content_length.is_some() && content_length.unwrap() > 5 * 1024 * 1024 {
            self.download_chunked(url, destination, content_length.unwrap()).await
        } else {
            self.download_sequential(url, destination).await
        }
    }

    async fn download_sequential(&self, url: &str, destination: &Path) -> Result<()> {
        let resp = self.client.get(url).send().await?;
        let bytes = resp.bytes().await?;
        std::fs::write(destination, bytes)?;
        Ok(())
    }

    async fn download_chunked(&self, url: &str, destination: &Path, total_size: u64) -> Result<()> {
        let chunk_size = total_size / self.max_concurrent_chunks;
        let file = Arc::new(Mutex::new(OpenOptions::new().create(true).write(true).open(destination)?));
        file.lock().await.set_len(total_size)?;

        let mut tasks = Vec::new();
        for i in 0..self.max_concurrent_chunks {
            let start = i * chunk_size;
            let end = if i == self.max_concurrent_chunks - 1 { total_size - 1 } else { start + chunk_size - 1 };
            let client = self.client.clone();
            let file_clone = Arc::clone(&file);
            let url = url.to_string();

            tasks.push(tokio::spawn(async move {
                let resp = client.get(&url).header(RANGE, format!("bytes={}-{}", start, end)).send().await?;
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

    pub async fn check_endpoint_availability(&self, url: &str) -> bool {
        self.client.head(url).send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }
}