//! Dependency injection for health checks.

use std::sync::Arc;

use zksync_node_framework::Resource;

use crate::AppHealthCheck;

/// A resource that provides [`AppHealthCheck`] to the service.
#[derive(Debug, Clone, Default)]
pub struct AppHealthCheckResource(pub Arc<AppHealthCheck>);

impl Resource for AppHealthCheckResource {
    fn name() -> String {
        "common/app_health_check".into()
    }
}
