use std::{fmt, sync::Arc};

// Public re-exports from external crate to minimize the required dependencies.
pub use zksync_health_check::{CheckHealth, ReactiveHealthCheck};

use crate::resource::Resource;

/// Wrapper for a generic health check.
#[derive(Clone)]
pub struct HealthCheckResource(Arc<dyn CheckHealth>);

impl HealthCheckResource {
    pub fn new(check: impl CheckHealth + 'static) -> Self {
        Self(Arc::new(check))
    }
}

impl fmt::Debug for HealthCheckResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HealthCheckResource")
            .field("name", &self.0.name())
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
