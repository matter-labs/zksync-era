use std::sync::Arc;

use zksync_circuit_breaker::CircuitBreakers;

use crate::resource::Resource;

#[derive(Debug, Clone, Default)]
pub struct CircuitBreakersResource {
    pub replication_lag_limit_sec: Option<u32>,
    pub breakers: Arc<CircuitBreakers>,
}

impl CircuitBreakersResource {
    pub fn set_replication_lag_limit_sec(&mut self, limit: u32) {
        self.replication_lag_limit_sec = Some(limit);
    }
}

impl Resource for CircuitBreakersResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/circuit_breakers".into()
    }
}
