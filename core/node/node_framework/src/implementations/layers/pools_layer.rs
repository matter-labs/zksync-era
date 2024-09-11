use zksync_config::configs::{DatabaseSecrets, PostgresConfig};
use zksync_dal::{ConnectionPool, Core};

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource, ReplicaPool},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Builder for the [`PoolsLayer`].
#[derive(Debug)]
pub struct PoolsLayerBuilder {
    config: PostgresConfig,
    with_master: bool,
    with_replica: bool,
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

    /// Builds the [`PoolsLayer`] with the provided configuration.
    pub fn build(self) -> PoolsLayer {
        PoolsLayer {
            config: self.config,
            secrets: self.secrets,
            with_master: self.with_master,
            with_replica: self.with_replica,
        }
    }
}

/// Wiring layer for connection pools.
/// During wiring, also prepares the global configuration for the connection pools.
///
/// ## Adds resources
///
/// - `PoolResource::<MasterPool>` (if master pool is enabled)
/// - `PoolResource::<ReplicaPool>` (if replica pool is enabled)
#[derive(Debug)]
pub struct PoolsLayer {
    config: PostgresConfig,
    secrets: DatabaseSecrets,
    with_master: bool,
    with_replica: bool,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub master_pool: Option<PoolResource<MasterPool>>,
    pub replica_pool: Option<PoolResource<ReplicaPool>>,
}

#[async_trait::async_trait]
impl WiringLayer for PoolsLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "pools_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        if !self.with_master && !self.with_replica {
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

        let master_pool = if self.with_master {
            let pool_size = self.config.max_connections()?;
            let pool_size_master = self.config.max_connections_master().unwrap_or(pool_size);

            Some(PoolResource::<MasterPool>::new(
                self.secrets.master_url()?,
                pool_size_master,
                None,
                None,
            ))
        } else {
            None
        };

        let replica_pool = if self.with_replica {
            // We're most interested in setting acquire / statement timeouts for the API server, which puts the most load
            // on Postgres.
            Some(PoolResource::<ReplicaPool>::new(
                self.secrets.replica_url()?,
                self.config.max_connections()?,
                self.config.statement_timeout(),
                self.config.acquire_timeout(),
            ))
        } else {
            None
        };

        Ok(Output {
            master_pool,
            replica_pool,
        })
    }
}
