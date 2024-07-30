mod error;
mod processor;

use std::{net::SocketAddr, sync::Arc};

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;

pub async fn run_server(
    config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    tracing::debug!("Starting external prover API server on {bind_address}");
    let app = create_proof_processing_router(blob_store, connection_pool, config, commitment_mode);

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding external prover API server to {bind_address}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop signal sender for external prover API server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, external prover API server is shutting down");
        })
        .await
        .context("External prover API server failed")?;
    tracing::info!("External prover API server shut down");
    Ok(())
}
