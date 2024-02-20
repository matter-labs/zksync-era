use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use zksync_dal::{connection::ConnectionPoolBuilder, ConnectionPool};

use crate::resource::Resource;

/// Represents a connection pool to the master database.
#[derive(Debug, Clone)]
pub struct MasterPoolResource {
    connections_count: Arc<AtomicU32>,
    builder: ConnectionPoolBuilder,
}

impl Resource for MasterPoolResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/master_pool".into()
    }
}

impl MasterPoolResource {
    pub fn new(builder: ConnectionPoolBuilder) -> Self {
        Self {
            connections_count: Arc::new(AtomicU32::new(0)),
            builder,
        }
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool> {
        let result = self.builder.build().await;

        if result.is_ok() {
            self.connections_count
                .fetch_add(self.builder.max_size(), Ordering::Relaxed);
            let total_connections = self.connections_count.load(Ordering::Relaxed);
            tracing::info!(
                "Created a new master pool. Master pool total connections count: {total_connections}"
            );
        }

        result
    }

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool> {
        let result = self.builder.build_singleton().await;

        if result.is_ok() {
            self.connections_count.fetch_add(1, Ordering::Relaxed);
            let total_connections = self.connections_count.load(Ordering::Relaxed);
            tracing::info!(
                "Created a new master pool. Master pool total connections count: {total_connections}"
            );
        }

        result
    }
}

/// Represents a connection pool to the replica database.
#[derive(Debug, Clone)]
pub struct ReplicaPoolResource {
    connections_count: Arc<AtomicU32>,
    builder: ConnectionPoolBuilder,
}

impl Resource for ReplicaPoolResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/replica_pool".into()
    }
}

impl ReplicaPoolResource {
    pub fn new(builder: ConnectionPoolBuilder) -> Self {
        Self {
            connections_count: Arc::new(AtomicU32::new(0)),
            builder,
        }
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool> {
        let result = self.builder.build().await;

        if result.is_ok() {
            self.connections_count
                .fetch_add(self.builder.max_size(), Ordering::Relaxed);
            let total_connections = self.connections_count.load(Ordering::Relaxed);
            tracing::info!(
                "Created a new replica pool. Master pool total connections count: {total_connections}"
            );
        }

        result
    }

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool> {
        let result = self.builder.build_singleton().await;

        if result.is_ok() {
            self.connections_count.fetch_add(1, Ordering::Relaxed);
            let total_connections = self.connections_count.load(Ordering::Relaxed);
            tracing::info!(
                "Created a new replica pool. Master pool total connections count: {total_connections}"
            );
        }

        result
    }
}

/// Represents a connection pool to the prover database.
#[derive(Debug, Clone)]
pub struct ProverPoolResource {
    connections_count: Arc<AtomicU32>,
    builder: ConnectionPoolBuilder,
}

impl Resource for ProverPoolResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/prover_pool".into()
    }
}

impl ProverPoolResource {
    pub fn new(builder: ConnectionPoolBuilder) -> Self {
        Self {
            connections_count: Arc::new(AtomicU32::new(0)),
            builder,
        }
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool> {
        let result = self.builder.build().await;

        if result.is_ok() {
            self.connections_count
                .fetch_add(self.builder.max_size(), Ordering::Relaxed);
            let total_connections = self.connections_count.load(Ordering::Relaxed);
            tracing::info!(
                "Created a new prover pool. Master pool total connections count: {total_connections}"
            );
        }

        result
    }

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool> {
        let result = self.builder.build_singleton().await;

        if result.is_ok() {
            self.connections_count.fetch_add(1, Ordering::Relaxed);
            let total_connections = self.connections_count.load(Ordering::Relaxed);
            tracing::info!(
                "Created a new prover pool. Master pool total connections count: {total_connections}"
            );
        }

        result
    }
}
