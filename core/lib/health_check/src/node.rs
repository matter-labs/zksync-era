//! Dependency injection for health checks.

use zksync_node_framework::Resource;

use crate::AppHealthCheck;

impl Resource for AppHealthCheck {
    fn name() -> String {
        "common/app_health_check".into()
    }
}
