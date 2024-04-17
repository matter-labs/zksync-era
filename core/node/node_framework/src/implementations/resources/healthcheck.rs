use std::sync::Arc;

use zksync_health_check::AppHealthCheck;
// Public re-exports from external crate to minimize the required dependencies.
pub use zksync_health_check::{CheckHealth, ReactiveHealthCheck};

use crate::resource::Resource;

#[derive(Debug, Clone, Default)]
pub struct AppHealthCheckResource(pub Arc<AppHealthCheck>);

impl Resource for AppHealthCheckResource {
    fn name() -> String {
        "common/app_health_check".into()
    }
}
