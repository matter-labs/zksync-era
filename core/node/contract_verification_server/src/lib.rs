use std::net::SocketAddr;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::ConnectionPool;

use self::api_decl::RestApi;

mod api_decl;
mod api_impl;
mod cache;
mod metrics;
#[cfg(test)]
mod tests;

pub async fn start_server(
    master_connection_pool: ConnectionPool<zksync_dal::Core>,
    replica_connection_pool: ConnectionPool<zksync_dal::Core>,
    bind_address: SocketAddr,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let api = RestApi::new(master_connection_pool, replica_connection_pool).into_router();

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .context("Cannot bind to the specified address")?;
    axum::serve(listener, api)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop request sender for contract verification server was dropped without sending a signal");
            }
            tracing::info!("Stop request received, contract verification server is shutting down");
        })
        .await
        .context("Contract verification handler server failed")?;
    tracing::info!("Contract verification handler server shut down");
    Ok(())
}
