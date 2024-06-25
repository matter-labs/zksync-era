use std::sync::Arc;

use zksync_config::configs::{DatabaseSecrets, PostgresConfig};
use zksync_dal::{ConnectionPool, Core};
use zksync_db_connection::healthcheck::ConnectionPoolHealthCheck;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource, ProverPool, ReplicaPool},
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Builder for the [`PoolsLayer`].
#[derive(Debug)]
pub struct PoolsLayerBuilder {
    config: PostgresConfig,
    with_master: bool,
    with_replica: bool,
    with_prover: bool,
    secrets: DatabaseSecrets,
}

impl PoolsLayerBuilder {
    /// Creates a new builder with the provided configuration and secrets.
    /// By default, no pulls are enabled.
    pub fn empty(config: PostgresConfig, database_secrets: DatabaseSecrets) -> Self {
        Self {
            config,
            with_master: false,
            with_replica: false,
            with_prover: false,
            secrets: database_secrets,
        }
    }

    /// Allows to enable the master pool.
    pub fn with_master(mut self, with_master: bool) -> Self {
        self.with_master = with_master;
        self
    }

    /// Allows to enable the replica pool.
    pub fn with_replica(mut self, with_replica: bool) -> Self {
        self.with_replica = with_replica;
        self
    }

    /// Allows to enable the prover pool.
    pub fn with_prover(mut self, with_prover: bool) -> Self {
        self.with_prover = with_prover;
        self
    }

    /// Builds the [`PoolsLayer`] with the provided configuration.
    pub fn build(self) -> PoolsLayer {
        PoolsLayer {
            config: self.config,
            secrets: self.secrets,
            with_master: self.with_master,
            with_replica: self.with_replica,
            with_prover: self.with_prover,
        }
    }
}

/// Wiring layer for connection pools.
/// During wiring, also prepares the global configuration for the connection pools.
///
/// ## Requests resources
///
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds resources
///
/// - `PoolResource::<MasterPool>` (if master pool is enabled)
/// - `PoolResource::<ReplicaPool>` (if replica pool is enabled)
/// - `PoolResource::<ProverPool>` (if prover pool is enabled)
#[derive(Debug)]
pub struct PoolsLayer {
    config: PostgresConfig,
    secrets: DatabaseSecrets,
    with_master: bool,
    with_replica: bool,
    with_prover: bool,
}

#[async_trait::async_trait]
impl WiringLayer for PoolsLayer {
    fn layer_name(&self) -> &'static str {
        "pools_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        if !self.with_master && !self.with_replica && !self.with_prover {
            return Err(WiringError::Configuration(
                "At least one pool should be enabled".to_string(),
            ));
        }

        if self.with_master || self.with_replica {
            if let Some(threshold) = self.config.slow_query_threshold() {
                ConnectionPool::<Core>::global_config().set_slow_query_threshold(threshold)?;
            }
            if let Some(threshold) = self.config.long_connection_threshold() {
                ConnectionPool::<Core>::global_config().set_long_connection_threshold(threshold)?;
            }
        }

        if self.with_master {
            let pool_size = self.config.max_connections()?;
            let pool_size_master = self.config.max_connections_master().unwrap_or(pool_size);

            context.insert_resource(PoolResource::<MasterPool>::new(
                self.secrets.master_url()?,
                pool_size_master,
                None,
                None,
            ))?;
        }

        if self.with_replica {
            // We're most interested in setting acquire / statement timeouts for the API server, which puts the most load
            // on Postgres.
            context.insert_resource(PoolResource::<ReplicaPool>::new(
                self.secrets.replica_url()?,
                self.config.max_connections()?,
                self.config.statement_timeout(),
                self.config.acquire_timeout(),
            ))?;
        }

        if self.with_prover {
            context.insert_resource(PoolResource::<ProverPool>::new(
                self.secrets.prover_url()?,
                self.config.max_connections()?,
                None,
                None,
            ))?;
        }

        // Insert health checks for the core pool.
        let connection_pool = if self.with_replica {
            context
                .get_resource::<PoolResource<ReplicaPool>>()
                .await?
                .get()
                .await?
        } else {
            context
                .get_resource::<PoolResource<MasterPool>>()
                .await?
                .get()
                .await?
        };
        let db_health_check = ConnectionPoolHealthCheck::new(connection_pool);
        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health
            .insert_custom_component(Arc::new(db_health_check))
            .map_err(WiringError::internal)?;

        Ok(())
    }
}
