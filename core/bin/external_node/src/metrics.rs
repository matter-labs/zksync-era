use std::time::Duration;

use tokio::sync::watch;
use vise::{EncodeLabelSet, Gauge, Info, Metrics};
use zksync_dal::{ConnectionPool, Core, CoreDal};

use crate::{config::ExternalNodeConfig, metadata::SERVER_VERSION};

/// Immutable EN parameters that affect multiple components.
#[derive(Debug, Clone, Copy, EncodeLabelSet)]
struct ExternalNodeInfo {
    server_version: &'static str,
    l1_chain_id: u64,
    l2_chain_id: u64,
    /// Size of the main Postgres connection pool.
    postgres_pool_size: u32,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node")]
pub(crate) struct ExternalNodeMetrics {
    /// General information about the external node.
    info: Info<ExternalNodeInfo>,
    /// Current protocol version.
    protocol_version: Gauge<u64>,
}

impl ExternalNodeMetrics {
    pub(crate) fn observe_config(&self, config: &ExternalNodeConfig) {
        let info = ExternalNodeInfo {
            server_version: SERVER_VERSION,
            l1_chain_id: config.required.l1_chain_id.0,
            l2_chain_id: config.required.l2_chain_id.as_u64(),
            postgres_pool_size: config.postgres.max_connections,
        };
        tracing::info!("Setting general node information: {info:?}");

        if self.info.set(info).is_err() {
            tracing::warn!(
                "General information is already set for the external node: {:?}, was attempting to set {info:?}",
                self.info.get()
            );
        }
    }

    pub(crate) async fn run_protocol_version_updates(
        &self,
        pool: ConnectionPool<Core>,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        const QUERY_INTERVAL: Duration = Duration::from_secs(10);

        while !*stop_receiver.borrow_and_update() {
            let maybe_protocol_version = pool
                .connection()
                .await?
                .protocol_versions_dal()
                .last_used_version_id()
                .await;
            if let Some(version) = maybe_protocol_version {
                self.protocol_version.set(version as u64);
            }

            tokio::time::timeout(QUERY_INTERVAL, stop_receiver.changed())
                .await
                .ok();
        }
        Ok(())
    }
}

#[vise::register]
pub(crate) static EN_METRICS: vise::Global<ExternalNodeMetrics> = vise::Global::new();
