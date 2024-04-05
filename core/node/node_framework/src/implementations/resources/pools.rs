use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use prover_dal::Prover;
use zksync_dal::{ConnectionPool, Core};
use zksync_db_connection::connection_pool::ConnectionPoolBuilder;

use crate::resource::Resource;

/// Represents a connection pool to the master database.
#[derive(Debug, Clone)]
pub struct MasterPoolResource {
    connections_count: Arc<AtomicU32>,
    builder: ConnectionPoolBuilder<Core>,
}

impl Resource for MasterPoolResource {
    fn name() -> String {
        "common/master_pool".into()
    }
}

impl MasterPoolResource {
    pub fn new(builder: ConnectionPoolBuilder<Core>) -> Self {
        Self {
            connections_count: Arc::new(AtomicU32::new(0)),
            builder,
        }
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool<Core>> {
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

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool<Core>> {
        self.get_custom(1).await
    }

    pub async fn get_custom(&self, size: u32) -> anyhow::Result<ConnectionPool<Core>> {
        let result = self.builder.clone().set_max_size(size).build().await;

        if result.is_ok() {
            let old_count = self.connections_count.fetch_add(size, Ordering::Relaxed);
            let total_connections = old_count + size;
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
    builder: ConnectionPoolBuilder<Core>,
}

impl Resource for ReplicaPoolResource {
    fn name() -> String {
        "common/replica_pool".into()
    }
}

impl ReplicaPoolResource {
    pub fn new(builder: ConnectionPoolBuilder<Core>) -> Self {
        Self {
            connections_count: Arc::new(AtomicU32::new(0)),
            builder,
        }
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool<Core>> {
        let result = self.builder.build().await;

        if result.is_ok() {
            self.connections_count
                .fetch_add(self.builder.max_size(), Ordering::Relaxed);
            let total_connections = self.connections_count.load(Ordering::Relaxed);
            tracing::info!(
                "Created a new replica pool. Replica pool total connections count: {total_connections}"
            );
        }

        result
    }

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool<Core>> {
        self.get_custom(1).await
    }

    pub async fn get_custom(&self, size: u32) -> anyhow::Result<ConnectionPool<Core>> {
        let result = self.builder.clone().set_max_size(size).build().await;

        if result.is_ok() {
            let old_count = self.connections_count.fetch_add(size, Ordering::Relaxed);
            let total_connections = old_count + size;
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
    builder: ConnectionPoolBuilder<Prover>,
}

impl Resource for ProverPoolResource {
    fn name() -> String {
        "common/prover_pool".into()
    }
}

impl ProverPoolResource {
    pub fn new(builder: ConnectionPoolBuilder<Prover>) -> Self {
        Self {
            connections_count: Arc::new(AtomicU32::new(0)),
            builder,
        }
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool<Prover>> {
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

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool<Prover>> {
        self.get_custom(1).await
    }

    pub async fn get_custom(&self, size: u32) -> anyhow::Result<ConnectionPool<Prover>> {
        let result = self.builder.clone().set_max_size(size).build().await;

        if result.is_ok() {
            let old_count = self.connections_count.fetch_add(size, Ordering::Relaxed);
            let total_connections = old_count + size;
            tracing::info!(
                "Created a new prover pool. Prover pool total connections count: {total_connections}"
            );
        }

        result
    }
}
