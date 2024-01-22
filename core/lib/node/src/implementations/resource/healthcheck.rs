use std::{fmt, sync::Arc};

// Public re-exports from external crate to minimize the required dependencies.
pub use zksync_health_check::{CheckHealth, ReactiveHealthCheck};

use crate::resource::Resource;

#[derive(Clone)]
pub struct HealthCheckResource(Arc<dyn CheckHealth>);

impl fmt::Debug for HealthCheckResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HealthCheckResource")
            .finish_non_exhaustive()
    }
}

impl Resource for HealthCheckResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/health_check".into()
    }
}

impl AsRef<dyn CheckHealth> for HealthCheckResource {
    fn as_ref(&self) -> &dyn CheckHealth {
        self.0.as_ref()
    }
}
