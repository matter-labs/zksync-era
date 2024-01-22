use zksync_dal::{connection::ConnectionPoolBuilder, ConnectionPool};

use crate::resource::Resource;

/// Represents a connection pool to the master database.
#[derive(Debug, Clone)]
pub struct MasterPoolResource(ConnectionPoolBuilder);

impl Resource for MasterPoolResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/master_pool".into()
    }
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
    fn resource_id() -> crate::resource::ResourceId {
        "common/replica_pool".into()
    }
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
    fn resource_id() -> crate::resource::ResourceId {
        "common/prover_pool".into()
    }
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
