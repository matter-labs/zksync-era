use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::configs::api::ContractVerificationApiConfig;
use zksync_dal::ConnectionPool;

use self::api_decl::RestApi;

mod api_decl;
mod api_impl;
mod metrics;

pub async fn start_server(
    master_connection_pool: ConnectionPool<zksync_dal::Core>,
    replica_connection_pool: ConnectionPool<zksync_dal::Core>,
    api_config: ContractVerificationApiConfig,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = api_config.bind_addr();
    let api = RestApi::new(master_connection_pool, replica_connection_pool).into_router();

    axum::Server::bind(&bind_address)
        .serve(api.into_make_service())
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop signal sender for contract verification server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, contract verification server is shutting down");
        })
        .await
        .context("Contract verification handler server failed")?;
    tracing::info!("Contract verification handler server shut down");
    Ok(())
}
