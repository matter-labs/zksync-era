use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::sync::Mutex;
use zksync_db_connection::connection_pool::ConnectionPoolBuilder;
use zksync_node_framework::resource::Resource;
use zksync_types::url::SensitiveUrl;

use crate::{ConnectionPool, Core};

/// Represents a connection pool to a certain kind of database.
#[derive(Debug, Clone)]
pub struct PoolResource<P: PoolKind> {
    connections_count: Arc<AtomicU32>,
    url: SensitiveUrl,
    max_connections: u32,
    statement_timeout: Option<Duration>,
    acquire_timeout: Option<Duration>,
    unbound_pool: Arc<Mutex<Option<ConnectionPool<P::DbMarker>>>>,
    _kind: std::marker::PhantomData<P>,
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
        acquire_timeout: Option<Duration>,
    ) -> Self {
        Self {
            connections_count: Arc::new(AtomicU32::new(0)),
            url,
            max_connections,
            statement_timeout,
            acquire_timeout,
            unbound_pool: Arc::new(Mutex::new(None)),
            _kind: std::marker::PhantomData,
        }
    }

    fn builder(&self) -> ConnectionPoolBuilder<P::DbMarker> {
        let mut builder = ConnectionPool::builder(self.url.clone(), self.max_connections);
        builder.set_statement_timeout(self.statement_timeout);
        builder.set_acquire_timeout(self.acquire_timeout);
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
        self.build(|builder| {
            builder.set_max_size(size);
        })
        .await
    }

    pub async fn build<F>(&self, build_fn: F) -> anyhow::Result<ConnectionPool<P::DbMarker>>
    where
        F: FnOnce(&mut ConnectionPoolBuilder<P::DbMarker>),
    {
        let mut builder = self.builder();
        build_fn(&mut builder);
        let size = builder.max_size();
        let result = builder.build().await;

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
