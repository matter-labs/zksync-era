use anyhow::Context as _;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Method;

use zksync_config::configs::ProverGroupConfig;
use zksync_utils::http_with_retries::send_request_with_retries;

pub async fn get_region(prover_group_config: &ProverGroupConfig) -> anyhow::Result<String> {
    if let Some(region) = &prover_group_config.region_override {
        return Ok(region.clone());
    }
    let url = &prover_group_config.region_read_url;
    fetch_from_url(url).await.context("fetch_from_url()")
}

pub async fn get_zone(prover_group_config: &ProverGroupConfig) -> anyhow::Result<String> {
    if let Some(zone) = &prover_group_config.zone_override {
        return Ok(zone.clone());
    }
    let url = &prover_group_config.zone_read_url;
    let data = fetch_from_url(url).await.context("fetch_from_url()")?;
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
    use zksync_config::configs::ProverGroupConfig;

    use crate::region_fetcher::{get_region, get_zone, parse_zone};

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

    #[tokio::test]
    async fn test_get_region_with_override() {
        let config = ProverGroupConfig {
            group_0_circuit_ids: vec![],
            group_1_circuit_ids: vec![],
            group_2_circuit_ids: vec![],
            group_3_circuit_ids: vec![],
            group_4_circuit_ids: vec![],
            group_5_circuit_ids: vec![],
            group_6_circuit_ids: vec![],
            group_7_circuit_ids: vec![],
            group_8_circuit_ids: vec![],
            group_9_circuit_ids: vec![],
            region_override: Some("us-central-1".to_string()),
            region_read_url: "".to_string(),
            zone_override: Some("us-central-1-b".to_string()),
            zone_read_url: "".to_string(),
            synthesizer_per_gpu: 0,
        };

        assert_eq!("us-central-1", get_region(&config).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_zone_with_override() {
        let config = ProverGroupConfig {
            group_0_circuit_ids: vec![],
            group_1_circuit_ids: vec![],
            group_2_circuit_ids: vec![],
            group_3_circuit_ids: vec![],
            group_4_circuit_ids: vec![],
            group_5_circuit_ids: vec![],
            group_6_circuit_ids: vec![],
            group_7_circuit_ids: vec![],
            group_8_circuit_ids: vec![],
            group_9_circuit_ids: vec![],
            region_override: Some("us-central-1".to_string()),
            region_read_url: "".to_string(),
            zone_override: Some("us-central-1-b".to_string()),
            zone_read_url: "".to_string(),
            synthesizer_per_gpu: 0,
        };
        assert_eq!("us-central-1-b", get_zone(&config).await.unwrap());
    }
}
