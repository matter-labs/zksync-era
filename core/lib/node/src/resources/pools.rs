use zksync_dal::{connection::ConnectionPoolBuilder, ConnectionPool};

use super::Resource;

pub const RESOURCE_NAME: &str = "common/postgres_pools";

#[derive(Debug, Clone)]
struct Pools {
    master_pool: Option<ConnectionPoolBuilder>,
    replica_pool: Option<ConnectionPoolBuilder>,
    prover_pool: Option<ConnectionPoolBuilder>,
}

impl Resource for Pools {}

impl Pools {
    pub fn with_master_pool(mut self, master_pool: ConnectionPoolBuilder) -> Self {
        self.master_pool = Some(master_pool);
        self
    }

    pub fn with_replica_pool(mut self, replica_pool: ConnectionPoolBuilder) -> Self {
        self.replica_pool = Some(replica_pool);
        self
    }

    pub fn with_prover_pool(mut self, prover_pool: ConnectionPoolBuilder) -> Self {
        self.prover_pool = Some(prover_pool);
        self
    }

    pub async fn master_pool(&self) -> anyhow::Result<ConnectionPool> {
        self.master_pool
            .as_ref()
            .map(|builder| builder.build())
            .expect("master pool is not initialized")
            .await
    }

    pub async fn master_pool_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.master_pool
            .as_ref()
            .map(|builder| builder.build_singleton())
            .expect("master pool is not initialized")
            .await
    }

    pub async fn replica_pool(&self) -> anyhow::Result<ConnectionPool> {
        self.replica_pool
            .as_ref()
            .map(|builder| builder.build())
            .expect("replica pool is not initialized")
            .await
    }

    pub async fn replica_pool_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.replica_pool
            .as_ref()
            .map(|builder| builder.build_singleton())
            .expect("replica pool is not initialized")
            .await
    }

    pub async fn prover_pool(&self) -> anyhow::Result<ConnectionPool> {
        self.prover_pool
            .as_ref()
            .map(|builder| builder.build())
            .expect("prover pool is not initialized")
            .await
    }

    pub async fn prover_pool_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.prover_pool
            .as_ref()
            .map(|builder| builder.build_singleton())
            .expect("prover pool is not initialized")
            .await
    }
}
