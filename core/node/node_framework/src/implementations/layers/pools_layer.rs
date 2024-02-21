use zksync_config::configs::PostgresConfig;
use zksync_dal::ConnectionPool;

use crate::{
    implementations::resources::pools::{
        MasterPoolResource, ProverPoolResource, ReplicaPoolResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct PoolsLayerBuilder {
    config: PostgresConfig,
    with_master: bool,
    with_replica: bool,
    with_prover: bool,
}

impl PoolsLayerBuilder {
    pub fn empty(config: PostgresConfig) -> Self {
        Self {
            config,
            with_master: false,
            with_replica: false,
            with_prover: false,
        }
    }

    pub fn with_master(mut self, with_master: bool) -> Self {
        self.with_master = with_master;
        self
    }

    pub fn with_replica(mut self, with_replica: bool) -> Self {
        self.with_replica = with_replica;
        self
    }

    pub fn with_prover(mut self, with_prover: bool) -> Self {
        self.with_prover = with_prover;
        self
    }

    pub fn build(self) -> PoolsLayer {
        PoolsLayer {
            config: self.config,
            with_master: self.with_master,
            with_replica: self.with_replica,
            with_prover: self.with_prover,
        }
    }
}

#[derive(Debug)]
pub struct PoolsLayer {
    config: PostgresConfig,
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

        if self.with_master {
            let mut master_pool =
                ConnectionPool::builder(self.config.master_url()?, self.config.max_connections()?);
            master_pool.set_statement_timeout(self.config.statement_timeout());
            context.insert_resource(MasterPoolResource::new(master_pool))?;
        }

        if self.with_replica {
            let mut replica_pool =
                ConnectionPool::builder(self.config.replica_url()?, self.config.max_connections()?);
            replica_pool.set_statement_timeout(self.config.statement_timeout());
            context.insert_resource(ReplicaPoolResource::new(replica_pool))?;
        }

        if self.with_prover {
            let mut prover_pool =
                ConnectionPool::builder(self.config.prover_url()?, self.config.max_connections()?);
            prover_pool.set_statement_timeout(self.config.statement_timeout());
            context.insert_resource(ProverPoolResource::new(prover_pool))?;
        }

        Ok(())
    }
}
