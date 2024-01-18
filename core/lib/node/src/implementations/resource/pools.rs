use zksync_dal::{connection::ConnectionPoolBuilder, ConnectionPool};

use crate::resource::Resource;

/// Represents a connection pool to the master database.
#[derive(Debug, Clone)]
pub struct MasterPoolResource(ConnectionPoolBuilder);

impl Resource for MasterPoolResource {
    const RESOURCE_NAME: &'static str = "common/master_pool";
}

impl MasterPoolResource {
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

impl Resource for ReplicaPoolResource {
    const RESOURCE_NAME: &'static str = "common/replica_pool";
}

impl ReplicaPoolResource {
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

impl Resource for ProverPoolResource {
    const RESOURCE_NAME: &'static str = "common/prover_pool";
}

impl ProverPoolResource {
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
