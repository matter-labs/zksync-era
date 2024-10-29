use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_health_check::{CheckHealth, Health, HealthStatus};

/// General information about the node.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub version: String,
}

/// Health details for a node.
#[derive(Debug, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum NodeHealth {
    Initializing,
    Running(NodeInfo),
}

impl From<NodeHealth> for Health {
    fn from(details: NodeHealth) -> Self {
        let status = match &details {
            NodeHealth::Initializing => HealthStatus::Affected,
            NodeHealth::Running(_) => HealthStatus::Ready,
        };
        Self::from(status).with_details(details)
    }
}

#[async_trait]
impl CheckHealth for NodeHealth {
    fn name(&self) -> &'static str {
        "node"
    }

    async fn check_health(&self) -> Health {
        NodeHealth::Running(NodeInfo {
            version: "0.1.0-yolo".to_string(),
        })
        .into()
    }
}
