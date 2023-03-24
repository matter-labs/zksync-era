use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Method;

use zksync_config::configs::ProverGroupConfig;
use zksync_utils::http_with_retries::send_request_with_retries;

pub async fn get_region() -> String {
    let prover_group_config = ProverGroupConfig::from_env();
    let mut headers = HeaderMap::new();
    headers.insert("Metadata-Flavor", HeaderValue::from_static("Google"));
    let response = send_request_with_retries(
        &prover_group_config.region_read_url,
        5,
        Method::GET,
        Some(headers),
        None,
    )
    .await;
    response
        .unwrap_or_else(|_| {
            panic!(
                "Failed fetching response from url: {}",
                prover_group_config.region_read_url
            )
        })
        .text()
        .await
        .expect("Failed to read response as text")
}
