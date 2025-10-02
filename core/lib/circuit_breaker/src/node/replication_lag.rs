use std::{sync::Arc, time::Duration};

use zksync_dal::node::{PoolResource, ReplicaPool};
use zksync_node_framework::{FromContext, WiringError, WiringLayer};

use crate::{replication_lag::ReplicationLagChecker, CircuitBreakers};

/// Layer adding [`ReplicationLagChecker`] to circuit breakers.
#[derive(Debug)]
pub struct ReplicationLagCheckerLayer {
    pub replication_lag_limit: Option<Duration>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    circuit_breakers: Arc<CircuitBreakers>,
    replica_pool: PoolResource<ReplicaPool>,
}

#[async_trait::async_trait]
impl WiringLayer for ReplicationLagCheckerLayer {
    type Input = Input;
    type Output = ();

    fn layer_name(&self) -> &'static str {
        "circuit_breakers/replication_lag"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let checker = ReplicationLagChecker {
            pool: input.replica_pool.get().await?,
            replication_lag_limit: self.replication_lag_limit,
        };
        input.circuit_breakers.insert(Box::new(checker)).await;
        Ok(())
    }
}
