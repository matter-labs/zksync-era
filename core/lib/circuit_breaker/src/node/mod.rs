//! Dependency injection for circuit breakers.

use zksync_node_framework::{resource, Resource};

pub use self::{checker::CircuitBreakerCheckerLayer, replication_lag::ReplicationLagCheckerLayer};
use crate::CircuitBreakers;

mod checker;
mod replication_lag;

impl Resource<resource::Shared> for CircuitBreakers {
    fn name() -> String {
        "common/circuit_breakers".into()
    }
}
