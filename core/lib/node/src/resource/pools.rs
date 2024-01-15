use zksync_dal::{connection::ConnectionPoolBuilder, ConnectionPool};

use super::Resource;

/// Represents a connection pool to the master database.
#[derive(Debug, Clone)]
pub struct MasterPoolResource(ConnectionPoolBuilder);

impl Resource for MasterPoolResource {}

impl MasterPoolResource {
    pub const RESOURCE_NAME: &str = "common/master_pool";

    pub fn new(builder: ConnectionPoolBuilder) -> Self {
        Self(builder)
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool> {
        self.0.build().await
    }

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.0.build_singleton().await
    }
}

/// Represents a connection pool to the replica database.
#[derive(Debug, Clone)]
pub struct ReplicaPoolResource(ConnectionPoolBuilder);

impl Resource for ReplicaPoolResource {}

impl ReplicaPoolResource {
    pub const RESOURCE_NAME: &str = "common/replica_pool";

    pub fn new(builder: ConnectionPoolBuilder) -> Self {
        Self(builder)
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool> {
        self.0.build().await
    }

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.0.build_singleton().await
    }
}

/// Represents a connection pool to the prover database.
#[derive(Debug, Clone)]
pub struct ProverPoolResource(ConnectionPoolBuilder);

impl Resource for ProverPoolResource {}

impl ProverPoolResource {
    pub const RESOURCE_NAME: &str = "common/prover_pool";

    pub fn new(builder: ConnectionPoolBuilder) -> Self {
        Self(builder)
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool> {
        self.0.build().await
    }

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.0.build_singleton().await
    }
}
