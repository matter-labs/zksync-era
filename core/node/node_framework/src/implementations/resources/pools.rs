use std::{
    fmt,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use prover_dal::Prover;
use tokio::sync::Mutex;
use zksync_dal::{ConnectionPool, Core};
use zksync_db_connection::connection_pool::ConnectionPoolBuilder;
use zksync_types::url::SensitiveUrl;

use crate::resource::Resource;

/// Represents a connection pool to a certain kind of database.
#[derive(Clone)]
pub struct PoolResource<P: PoolKind> {
    connections_count: Arc<AtomicU32>,
    url: SensitiveUrl,
    max_connections: u32,
    statement_timeout: Option<Duration>,
    unbound_pool: Arc<Mutex<Option<ConnectionPool<P::DbMarker>>>>,
    _kind: std::marker::PhantomData<P>,
}

impl<P: PoolKind> fmt::Debug for PoolResource<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PoolResource")
            .field("connections_count", &self.connections_count)
            .field("url", &self.url)
            .field("max_connections", &self.max_connections)
            .field("statement_timeout", &self.statement_timeout)
            .field("unbound_pool", &self.unbound_pool)
            .finish_non_exhaustive()
    }
}

impl<P: PoolKind> Resource for PoolResource<P> {
    fn name() -> String {
        format!("common/{}_pool", P::kind_str())
    }
}

impl<P: PoolKind> PoolResource<P> {
    pub fn new(
        url: SensitiveUrl,
        max_connections: u32,
        statement_timeout: Option<Duration>,
    ) -> Self {
        Self {
            connections_count: Arc::new(AtomicU32::new(0)),
            url,
            max_connections,
            statement_timeout,
            unbound_pool: Arc::new(Mutex::new(None)),
            _kind: std::marker::PhantomData,
        }
    }

    fn builder(&self) -> ConnectionPoolBuilder<P::DbMarker> {
        let mut builder = ConnectionPool::builder(self.url.clone(), self.max_connections);
        builder.set_statement_timeout(self.statement_timeout);
        builder
    }

    pub async fn get(&self) -> anyhow::Result<ConnectionPool<P::DbMarker>> {
        let mut unbound_pool = self.unbound_pool.lock().await;
        if let Some(pool) = unbound_pool.as_ref() {
            tracing::info!(
                "Provided a new copy of an existing {} unbound pool",
                P::kind_str()
            );
            return Ok(pool.clone());
        }
        let pool = self.builder().build().await?;
        *unbound_pool = Some(pool.clone());

        let old_count = self
            .connections_count
            .fetch_add(self.max_connections, Ordering::Relaxed);
        let total_connections = old_count + self.max_connections;
        tracing::info!(
            "Created a new {} pool. Total connections count: {total_connections}",
            P::kind_str()
        );

        Ok(pool)
    }

    pub async fn get_singleton(&self) -> anyhow::Result<ConnectionPool<P::DbMarker>> {
        self.get_custom(1).await
    }

    pub async fn get_custom(&self, size: u32) -> anyhow::Result<ConnectionPool<P::DbMarker>> {
        let result = self.builder().set_max_size(size).build().await;

        if result.is_ok() {
            let old_count = self.connections_count.fetch_add(size, Ordering::Relaxed);
            let total_connections = old_count + size;
            tracing::info!(
                "Created a new {} pool. Total connections count: {total_connections}",
                P::kind_str()
            );
        }

        result
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MasterPool {}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ReplicaPool {}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ProverPool {}

pub trait PoolKind: Clone + Sync + Send + 'static {
    type DbMarker: zksync_db_connection::connection::DbMarker;

    fn kind_str() -> &'static str;
}

impl PoolKind for MasterPool {
    type DbMarker = Core;

    fn kind_str() -> &'static str {
        "master"
    }
}

impl PoolKind for ReplicaPool {
    type DbMarker = Core;

    fn kind_str() -> &'static str {
        "replica"
    }
}

impl PoolKind for ProverPool {
    type DbMarker = Prover;

    fn kind_str() -> &'static str {
        "prover"
    }
}
