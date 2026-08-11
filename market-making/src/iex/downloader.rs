//! IEX Historical Data Downloader
//!
//! Downloads historical PCAP files from IEX Cloud's historical data service.
//! Source: https://iextrading.com/trading/market-data/#hist-download
//!
//! IEX provides daily PCAP files containing TOPS and DEEP market data
//! for all US equities. Files are named by date, e.g. "20240115_PCAP.gz".

use serde::Deserialize;
use reqwest::header::{ACCEPT_ENCODING, CACHE_CONTROL};
use std::collections::HashMap;
use std::fs::File;
use std::io::copy;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

/// Item returned by IEX HIST API (`https://iextrading.com/api/1.0/hist`).
#[derive(Debug, Clone, Deserialize)]
pub struct HistFeedEntry {
    pub link: String,
    pub date: String,
    pub feed: String,
    pub version: String,
    pub protocol: String,
    pub size: String,
}

/// Which IEX historical feed to download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IexFeed {
    /// TOPS — top-of-book quotes and trades
    Tops,
    /// DEEP — full depth-of-book price level updates
    Deep,
}

impl IexFeed {
    /// URL path segment for this feed.
    pub fn path_segment(&self) -> &'static str {
        match self {
            Self::Tops => "tops",
            Self::Deep => "deep",
        }
    }

    /// Filename suffix for this feed.
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Tops => "TOPS",
            Self::Deep => "DEEP",
        }
    }
}

/// IEX historical data downloader.
pub struct IexDownloader {
    /// Base URL for IEX historical data or API metadata endpoint
    base_url: String,
    /// Local directory to store downloaded files
    data_dir: PathBuf,
}

impl IexDownloader {
    /// Create a new downloader with the given data directory.
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            base_url: "https://iextrading.com/api/1.0/hist".to_string(),
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Create a downloader with a custom base URL (for testing or mirrors).
    pub fn with_base_url(base_url: &str, data_dir: impl AsRef<Path>) -> Self {
        Self {
            base_url: base_url.to_string(),
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Fetch metadata from the HIST API endpoint if `base_url` points to an API endpoint.
    pub fn fetch_hist_metadata(&self) -> Result<HashMap<String, Vec<HistFeedEntry>>, String> {
        let client = reqwest::blocking::Client::builder()
            .no_gzip()
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let response = client
            .get(&self.base_url)
            .send()
            .map_err(|e| format!("HTTP request to HIST API failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "HIST API returned status {} for {}",
                response.status(),
                self.base_url
            ));
        }

        let text = response
            .text()
            .map_err(|e| format!("Failed to read response text: {}", e))?;

        let entries: HashMap<String, Vec<HistFeedEntry>> = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse HIST API JSON response: {}", e))?;

        Ok(entries)
    }

    /// Resolve the direct download URL for a given date and feed.
    /// First attempts to lookup in the IEX HIST API index, then falls back to static URL builder.
    pub fn resolve_download_url(&self, date: &str, feed: IexFeed) -> Result<String, String> {
        if self.base_url.contains("/api/") {
            match self.fetch_hist_metadata() {
                Ok(metadata) => {
                    if let Some(entries) = metadata.get(date) {
                        let target_feed = feed.suffix().to_lowercase();
                        for entry in entries {
                            if entry.feed.to_lowercase() == target_feed {
                                return Ok(entry.link.clone());
                            }
                        }
                        return Err(format!(
                            "Feed {} not found in IEX HIST metadata for date {}",
                            feed.suffix(),
                            date
                        ));
                    } else {
                        return Err(format!("Date {} not found in IEX HIST metadata index", date));
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to query HIST API ({}), falling back to static URL builder",
                        e
                    );
                }
            }
        }

        Ok(self.build_url(date, feed))
    }

    /// Build the fallback download URL for a given date and feed.
    ///
    /// Date format: "YYYYMMDD"
    pub fn build_url(&self, date: &str, feed: IexFeed) -> String {
        let year = &date[..4];
        let month = &date[4..6];
        let day = &date[6..8];
        let base = if self.base_url.contains("/api/") {
            "https://www.nanex.net/iex"
        } else {
            &self.base_url
        };
        format!(
            "{}/{}/{}/{}/{}_{}_{}.pcap.gz",
            base,
            feed.path_segment(),
            year,
            month,
            day,
            date,
            feed.suffix()
        )
    }

    /// Compute the local file path for a given date and feed.
    pub fn local_path(&self, date: &str, feed: IexFeed) -> PathBuf {
        self.data_dir
            .join(format!("{}_{}.pcap.gz", date, feed.suffix()))
    }

    /// Download a historical IEX PCAP file for the given date and feed.
    ///
    /// Returns the path to the downloaded file, or an error.
    pub fn download(&self, date: &str, feed: IexFeed) -> Result<PathBuf, String> {
        let dest = self.local_path(date, feed);

        if dest.exists() {
            info!("File already exists, skipping download: {}", dest.display());
            return Ok(dest);
        }

        let url = self.resolve_download_url(date, feed)?;

        // Ensure data directory exists
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create data dir: {}", e))?;
        }

        info!("Downloading IEX {} data for {}: {}", feed.suffix(), date, url);

        // Build client with 10-minute timeout and disabled auto-gzip
        let client = reqwest::blocking::Client::builder()
            .no_gzip()
            .timeout(Duration::from_secs(6000))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let mut response = client
            .get(&url)
            .header(ACCEPT_ENCODING, "gzip")
            .header(CACHE_CONTROL, "no-transform")
            .send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Download failed with status {} for {}",
                response.status(),
                url
            ));
        }

        // Stream directly to disk rather than buffering in RAM
        let mut file = File::create(&dest)
            .map_err(|e| format!("Failed to create file {}: {}", dest.display(), e))?;

        let bytes_written = copy(&mut response, &mut file)
            .map_err(|e| format!("Failed to write download stream to {}: {}", dest.display(), e))?;

        info!("Downloaded {} bytes to {}", bytes_written, dest.display());

        Ok(dest)
    }

    /// Download multiple dates for a given feed. Returns paths for successful downloads.
    pub fn download_range(
        &self,
        dates: &[&str],
        feed: IexFeed,
    ) -> Vec<(String, Result<PathBuf, String>)> {
        dates
            .iter()
            .map(|&date| {
                let result = self.download(date, feed);
                if let Err(ref e) = result {
                    warn!("Failed to download {} for {}: {}", feed.suffix(), date, e);
                }
                (date.to_string(), result)
            })
            .collect()
    }
}

impl Default for IexDownloader {
    fn default() -> Self {
        Self::new("./data/iex")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_construction() {
        let dl = IexDownloader::new("/tmp/iex");
        let url = dl.build_url("20240115", IexFeed::Tops);
        assert!(url.contains("20240115"));
        assert!(url.contains("TOPS"));
    }

    #[test]
    fn local_path_construction() {
        let dl = IexDownloader::new("/tmp/iex");
        let path = dl.local_path("20240115", IexFeed::Deep);
        assert!(path.to_string_lossy().contains("20240115_DEEP.pcap.gz"));
    }
}