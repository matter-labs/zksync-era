use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Method;

use zksync_config::configs::ProverGroupConfig;
use zksync_utils::http_with_retries::send_request_with_retries;

pub async fn get_region() -> String {
    let prover_group_config = ProverGroupConfig::from_env();
    match prover_group_config.region_override {
        Some(region) => region,
        None => {
            let url = prover_group_config.region_read_url;
            fetch_from_url(url).await
        }
    }
}

pub async fn get_zone() -> String {
    let prover_group_config = ProverGroupConfig::from_env();
    match prover_group_config.zone_override {
        Some(zone) => zone,
        None => {
            let url = prover_group_config.zone_read_url;
            let data = fetch_from_url(url).await;
            parse_zone(&data)
        }
    }
}

async fn fetch_from_url(url: String) -> String {
    let mut headers = HeaderMap::new();
    headers.insert("Metadata-Flavor", HeaderValue::from_static("Google"));
    let response = send_request_with_retries(&url, 5, Method::GET, Some(headers), None).await;
    response
        .unwrap_or_else(|_| panic!("Failed fetching response from url: {}", url))
        .text()
        .await
        .expect("Failed to read response as text")
}

fn parse_zone(data: &str) -> String {
    let re = Regex::new(r"^projects/\d+/zones/(\w+-\w+-\w+)$").unwrap();
    if let Some(caps) = re.captures(data) {
        let zone = &caps[1];
        return zone.to_string();
    }
    panic!("failed to extract zone from: {}", data)
}

#[cfg(test)]
mod tests {
    use crate::region_fetcher::{get_region, get_zone, parse_zone};

    #[test]
    fn test_parse_zone() {
        let data = "projects/295056426491/zones/us-central1-a";
        let zone = parse_zone(data);
        assert_eq!(zone, "us-central1-a");
    }

    #[test]
    #[should_panic(expected = "failed to extract zone from: invalid data")]
    fn test_parse_zone_panic() {
        let data = "invalid data";
        let _ = parse_zone(data);
    }

    #[tokio::test]
    async fn test_get_region_with_override() {
        assert_eq!("us-central-1", get_region().await);
    }

    #[tokio::test]
    async fn test_get_zone_with_override() {
        assert_eq!("us-central-1-b", get_zone().await);
    }
}
