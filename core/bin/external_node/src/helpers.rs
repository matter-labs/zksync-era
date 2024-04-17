//! Miscellaneous helpers for the EN.

use zksync_health_check::{async_trait, CheckHealth, Health, HealthStatus};
use zksync_web3_decl::{client::BoxedL2Client, namespaces::EthNamespaceClient};

/// Main node health check.
#[derive(Debug)]
pub(crate) struct MainNodeHealthCheck(BoxedL2Client);

impl From<BoxedL2Client> for MainNodeHealthCheck {
    fn from(client: BoxedL2Client) -> Self {
        Self(client.for_component("main_node_health_check"))
    }
}

#[async_trait]
impl CheckHealth for MainNodeHealthCheck {
    fn name(&self) -> &'static str {
        "main_node_http_rpc"
    }

    async fn check_health(&self) -> Health {
        if let Err(err) = self.0.get_block_number().await {
            tracing::warn!("Health-check call to main node HTTP RPC failed: {err}");
            let details = serde_json::json!({
                "error": err.to_string(),
            });
            return Health::from(HealthStatus::NotReady).with_details(details);
        }
        HealthStatus::Ready.into()
    }
}
