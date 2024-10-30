use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_health_check::{CheckHealth, Health, HealthStatus};

const GIT_VERSION: &str = zksync_git_version_macro::build_git_revision!();
const GIT_BRANCH: &str = zksync_git_version_macro::build_git_branch!();

/// This struct implements a static health check describing node's version information.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeVersionInfo {
    git_version: String,
    git_branch: String,
}

impl Default for NodeVersionInfo {
    fn default() -> Self {
        Self {
            git_version: GIT_VERSION.to_string(),
            git_branch: GIT_BRANCH.to_string(),
        }
    }
}

impl From<&NodeVersionInfo> for Health {
    fn from(details: &NodeVersionInfo) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[async_trait]
impl CheckHealth for NodeVersionInfo {
    fn name(&self) -> &'static str {
        "version"
    }

    async fn check_health(&self) -> Health {
        self.into()
    }
}
