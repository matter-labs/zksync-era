use axum::{extract, extract::Json, routing::get, Router};
use tokio::sync::watch;
use zksync_config::configs::native_token_fetcher::NativeTokenFetcherConfig;

pub(crate) async fn run_server(
    mut stop_receiver: watch::Receiver<bool>,
    server_configs: &NativeTokenFetcherConfig,
) -> anyhow::Result<()> {
    let app = Router::new().route("/conversion_rate/:token_address", get(get_conversion_rate));

    let bind_address = if server_configs.host.starts_with("http://") {
        &server_configs.host[7..] // If it starts with "http://", strip the prefix
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

// basic handler that responds with a static string
async fn get_conversion_rate(extract::Path(_token_address): extract::Path<String>) -> Json<u64> {
    tracing::info!("Received request for conversion rate");
    Json(42)
}
