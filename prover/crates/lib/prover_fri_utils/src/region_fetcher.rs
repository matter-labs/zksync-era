use core::fmt;
use std::time::Duration;

use anyhow::Context;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use zksync_config::configs::fri_prover::CloudConnectionMode;

#[derive(Debug, Clone)]
pub struct RegionFetcher {
    gcp_zone_fetcher: GcpZoneFetcher,
    cloud_type: CloudConnectionMode,
    zone_url: String,
}

impl RegionFetcher {
    pub fn new(cloud_type: CloudConnectionMode, zone_url: String) -> Self {
        Self {
            gcp_zone_fetcher: GcpZoneFetcher::new(),
            cloud_type,
            zone_url,
        }
    }

    pub async fn get_zone(&self) -> anyhow::Result<Zone> {
        match self.cloud_type {
            CloudConnectionMode::GCP => self.gcp_zone_fetcher.get_zone(&self.zone_url).await,
            CloudConnectionMode::Local => Ok(Zone("local".to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone(String);

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Zone {
    pub fn new<T: ToString>(zone: T) -> Self {
        Self(zone.to_string())
    }
}

#[derive(Debug, Clone)]
struct GcpZoneFetcher {
    client: ClientWithMiddleware,
}

impl GcpZoneFetcher {
    pub fn new() -> Self {
        let retry_policy = ExponentialBackoff::builder()
            .base(2)
            .retry_bounds(Duration::from_secs(1), Duration::MAX)
            .build_with_max_retries(5);
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(
                RetryTransientMiddleware::new_with_policy(retry_policy)
                    .with_retry_log_level(tracing::Level::ERROR),
            )
            .build();
        Self { client }
    }

    pub async fn get_zone(&self, zone_url: &str) -> anyhow::Result<Zone> {
        let data = self
            .fetch_from_url(zone_url)
            .await
            .context("fetch_from_url()")?;
        Self::parse_zone(&data).context("parse_zone")
    }

    async fn fetch_from_url(&self, url: &str) -> anyhow::Result<String> {
        let mut headers = HeaderMap::new();
        headers.insert("Metadata-Flavor", HeaderValue::from_static("Google"));
        let response = self.client.get(url).headers(headers).send().await;
        response
            .map_err(|err| anyhow::anyhow!("Failed fetching response from url: {url}: {err:?}"))?
            .text()
            .await
            .context("Failed to read response as text")
    }

    fn parse_zone(data: &str) -> anyhow::Result<Zone> {
        // Statically provided Regex should always compile.
        let re = Regex::new(r"^projects/\d+/zones/(\w+-\w+-\w+)$").unwrap();
        if let Some(caps) = re.captures(data) {
            let zone = &caps[1];
            return Ok(Zone(zone.to_string()));
        }
        anyhow::bail!("failed to extract zone from: {data}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_zone() {
        let data = "projects/295056426491/zones/us-central1-a";
        let zone = GcpZoneFetcher::parse_zone(data).unwrap();
        assert_eq!(zone, Zone::new("us-central1-a"));
    }

    #[test]
    fn test_parse_zone_panic() {
        let data = "invalid data";
        assert!(GcpZoneFetcher::parse_zone(data).is_err());
    }
}
