//! IEX Historical Data Downloader
//!
//! Downloads historical PCAP files from IEX Cloud's historical data service.
//! Source: https://iextrading.com/trading/market-data/#hist-download
//!
//! IEX provides daily PCAP files containing TOPS and DEEP market data
//! for all US equities. Files are named by date, e.g. "20240115_PCAP.gz".

use std::path::{Path, PathBuf};
use tracing::{info, warn};

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
    /// Base URL for IEX historical data
    base_url: String,
    /// Local directory to store downloaded files
    data_dir: PathBuf,
}

impl IexDownloader {
    /// Create a new downloader with the given data directory.
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            base_url: "https://www.nanex.net/iex".to_string(),
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

    /// Build the download URL for a given date and feed.
    ///
    /// Date format: "YYYYMMDD"
    pub fn build_url(&self, date: &str, feed: IexFeed) -> String {
        let year = &date[..4];
        let month = &date[4..6];
        let day = &date[6..8];
        format!(
            "{}/{}/{}/{}/{}_{}_{}.pcap.gz",
            self.base_url,
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
        let url = self.build_url(date, feed);
        let dest = self.local_path(date, feed);

        if dest.exists() {
            info!("File already exists, skipping download: {}", dest.display());
            return Ok(dest);
        }

        // Ensure data directory exists
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create data dir: {}", e))?;
        }

        info!("Downloading IEX {} data for {}: {}", feed.suffix(), date, url);

        // Use blocking reqwest to download
        let response = reqwest::blocking::get(&url)
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Download failed with status {} for {}",
                response.status(),
                url
            ));
        }

        let bytes = response
            .bytes()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        std::fs::write(&dest, &bytes)
            .map_err(|e| format!("Failed to write file {}: {}", dest.display(), e))?;

        info!(
            "Downloaded {} bytes to {}",
            bytes.len(),
            dest.display()
        );

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