mod common;
mod disperser;

use std::{net::SocketAddr, str::FromStr};

use anyhow::Context as _;
use axum::{
    extract::Path,
    routing::{get, post},
    Router,
};
use eigenda_client::EigenDAClient;
use memstore::MemStore;
use request_processor::{ClientType, RequestProcessorNew};
use tokio::sync::watch;
use zksync_config::configs::da_client::eigen_da::EigenDAConfig;

mod blob_info;
mod eigenda_client;
mod errors;
mod memstore;
mod request_processor;

pub async fn run_server(
    config: EigenDAConfig,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let (bind_address, client) = match config {
        EigenDAConfig::MemStore(cfg) => {
            let bind_address = SocketAddr::from_str(&cfg.api_node_url)?;

            let client = MemStore::new(cfg);
            (bind_address, ClientType::Memory(client))
        }
        EigenDAConfig::Disperser(cfg) => {
            let bind_address = SocketAddr::from_str(&cfg.api_node_url)?;

            let client = EigenDAClient::new(cfg)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create EigenDA client: {:?}", e))?;
            (bind_address, ClientType::Disperser(client))
        }
    };

    tracing::debug!("Starting eigenda proxy on {bind_address}");

    // TODO: app should receive an impl instead of a struct
    let app = create_eigenda_proxy_router(client);

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding eigenda proxy to {bind_address}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop signal sender for eigenda proxy was dropped without sending a signal"
                );
            }
            tracing::info!("Stop signal received, eigenda proxy is shutting down");
        })
        .await
        .context("EigenDA proxy failed")?;
    tracing::info!("EigenDA proxy shut down");
    Ok(())
}

fn create_eigenda_proxy_router(client: ClientType) -> Router {
    let get_blob_id_processor = RequestProcessorNew::new(client);
    let pub_blob_id_processor = get_blob_id_processor.clone();
    let router = Router::new()
        .route(
            "/get/:l1_batch_number",
            get(move |blob_id: Path<String>| async move {
                get_blob_id_processor.get_blob_id(blob_id).await
            }),
        )
        .route(
            "/put/",
            post(move |blob_id: Path<String>| async move {
                pub_blob_id_processor.put_blob_id(blob_id).await
            }),
        );
    router
}

// fn create_eigenda_proxy_router(eigenda_client: EigenDAClient) -> Router {
//     let get_blob_id_processor = RequestProcessor::new(eigenda_client);
//     let pub_blob_id_processor = get_blob_id_processor.clone();
//     let router = Router::new()
//         .route(
//             "/get/:l1_batch_number",
//             get(move |blob_id: Path<String>| async move {
//                 get_blob_id_processor.get_blob_id(blob_id).await
//             }),
//         )
//         .route(
//             "/put/",
//             post(move |blob_id: Path<String>| async move {
//                 pub_blob_id_processor.put_blob_id(blob_id).await
//             }),
//         );
//     router
// }
