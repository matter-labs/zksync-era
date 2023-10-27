use std::time::Duration;

use tokio::sync::watch;

use zksync_dal::ConnectionPool;
use zksync_types::api::en::{Heartbeat, HeartbeatV1};
use zksync_web3_decl::{
    jsonrpsee::core::Error as RpcError,
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::EnNamespaceClient,
    RpcResult,
};

const SLEEP_INTERVAL: Duration = Duration::from_secs(5);

pub struct Heartbeater {
    name: String,
    client: HttpClient,
    pool: ConnectionPool,
    should_stop: watch::Receiver<bool>,
    version: semver::Version,
}

impl Heartbeater {
    pub fn new(
        name: String,
        url: &str,
        pool: ConnectionPool,
        should_stop: watch::Receiver<bool>,
        version: semver::Version,
    ) -> Self {
        let client = HttpClientBuilder::default()
            .build(url)
            .expect("Failed to create HTTP client");

        Self {
            name,
            client,
            pool,
            should_stop,
            version,
        }
    }

    pub async fn run(mut self) {
        loop {
            match self.run_inner().await {
                Ok(()) => return,
                Err(err @ RpcError::Transport(_) | err @ RpcError::RequestTimeout) => {
                    tracing::warn!("Following transport error occurred: {err}");
                    tracing::info!("Trying again after a delay");
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                }
                Err(err) => {
                    panic!("Unexpected error in heartbeater: {}", err);
                }
            }
        }
    }

    async fn run_inner(&mut self) -> RpcResult<()> {
        loop {
            let should_stop = *self.should_stop.borrow();

            if should_stop {
                return Ok(());
            }

            let protocol_version = self
                .pool
                .access_storage()
                .await
                .unwrap()
                .protocol_versions_dal()
                .last_version_id()
                .await
                .unwrap();

            let executed_l1_batch_number = self
                .pool
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .get_number_of_last_l1_batch_executed_on_eth()
                .await
                .unwrap()
                .unwrap();

            let last_known_l1_batch_number = self
                .pool
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .get_last_l1_batch_number_with_metadata()
                .await
                .unwrap();

            let heartbeat = Heartbeat::V1(HeartbeatV1 {
                name: self.name.clone(),
                server_version: self.version.clone(),
                protocol_version,
                executed_l1_batch_number,
                last_known_l1_batch_number,
            });

            self.client.send_heartbeat(heartbeat).await?;

            tokio::time::sleep(SLEEP_INTERVAL).await;
        }
    }
}
