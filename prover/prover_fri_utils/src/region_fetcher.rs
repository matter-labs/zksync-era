use std::net::ToSocketAddrs;

use anyhow::Context;
use regex::Regex;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Method,
};
use zksync_utils::http_with_retries::send_request_with_retries;

pub async fn get_zone(zone_url: &str) -> anyhow::Result<String> {
    // Check if zone URL can be resolved. If not, we assume that we're running locally.
    if zone_url.to_socket_addrs().is_err() {
        tracing::error!("Unable to resolve zone URL, assuming local environment");
        return Ok("local".to_string());
    }

    let data = fetch_from_url(zone_url).await.context("fetch_from_url()")?;
    parse_zone(&data).context("parse_zone")
}

async fn fetch_from_url(url: &str) -> anyhow::Result<String> {
    let mut headers = HeaderMap::new();
    headers.insert("Metadata-Flavor", HeaderValue::from_static("Google"));
    let response = send_request_with_retries(url, 5, Method::GET, Some(headers), None).await;
    response
        .map_err(|err| anyhow::anyhow!("Failed fetching response from url: {url}: {err:?}"))?
        .text()
        .await
        .context("Failed to read response as text")
}

fn parse_zone(data: &str) -> anyhow::Result<String> {
    // Statically provided Regex should always compile.
    let re = Regex::new(r"^projects/\d+/zones/(\w+-\w+-\w+)$").unwrap();
    if let Some(caps) = re.captures(data) {
        let zone = &caps[1];
        return Ok(zone.to_string());
    }
    anyhow::bail!("failed to extract zone from: {data}");
}

#[cfg(test)]
mod tests {
    use crate::region_fetcher::parse_zone;

    #[test]
    fn test_parse_zone() {
        let data = "projects/295056426491/zones/us-central1-a";
        let zone = parse_zone(data).unwrap();
        assert_eq!(zone, "us-central1-a");
    }

    #[test]
    fn test_parse_zone_panic() {
        let data = "invalid data";
        assert!(parse_zone(data).is_err());
    }
}
