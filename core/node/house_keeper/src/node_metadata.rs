use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_health_check::{CheckHealth, Health, HealthStatus};

const GIT_VERSION: &str = zksync_git_version_macro::build_git_revision!();
const GIT_BRANCH: &str = zksync_git_version_macro::build_git_branch!();

/// General information about the node.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub git_version: String,
    pub git_branch: String,
}

/// Health details for a node.
#[derive(Debug, Default, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum NodeHealth {
    #[default]
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
            git_version: GIT_VERSION.to_string(),
            git_branch: GIT_BRANCH.to_string(),
        })
        .into()
    }
}
