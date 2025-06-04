//! Dependency injection for health checks.

use zksync_node_framework::{resource, Resource};

use crate::AppHealthCheck;

impl Resource<resource::Shared> for AppHealthCheck {
    fn name() -> String {
        "common/app_health_check".into()
    }
}
