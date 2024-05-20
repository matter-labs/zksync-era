use std::str::FromStr;

use axum::{
    extract::{self, Json},
    routing::get,
    Router,
};
use bigdecimal::BigDecimal;
use tokio::sync::watch;
use zksync_config::configs::base_token_fetcher::BaseTokenFetcherConfig;

pub async fn run_server(
    mut stop_receiver: watch::Receiver<bool>,
    server_configs: &BaseTokenFetcherConfig,
) -> anyhow::Result<()> {
    let app = Router::new().route("/conversion_rate/:token_address", get(get_conversion_rate));

    let bind_address = if server_configs.host.starts_with("http://") {
        &server_configs.host[7..] // If it starts with "HTTP://", strip the prefix
    } else {
        &server_configs.host // Otherwise, return the original string
    };

    axum::Server::bind(&bind_address.parse().expect("Unable to parse socket address"))
        .serve(app.into_make_service())
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop signal sender for conversion rate API server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, conversion rate server is shutting down");
        })
        .await
        .expect("Conversion rate server failed");
    tracing::info!("Conversion rate server shut down");
    Ok(())
}

/// Basic handler that responds with a static string with the format expected by the token fetcher component
/// Returns 1 in the case of using ETH as the base token, a hardcoded 42 otherwise
async fn get_conversion_rate(extract::Path(token_address): extract::Path<String>) -> Json<String> {
    if token_address == "0x0000000000000000000000000000000000000000"
        || token_address == "0x0000000000000000000000000000000000000001"
    {
        return Json(BigDecimal::from(1).to_string());
    }
    tracing::info!("Received request for conversion rate");
    Json(BigDecimal::from_str("42.5").unwrap().to_string())
}
