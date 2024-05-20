use zksync_config::configs::{DatabaseSecrets, PostgresConfig};

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource, ProverPool, ReplicaPool},
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct PoolsLayerBuilder {
    config: PostgresConfig,
    with_master: bool,
    with_replica: bool,
    with_prover: bool,
    secrets: DatabaseSecrets,
}

impl PoolsLayerBuilder {
    pub fn empty(config: PostgresConfig, database_secrets: DatabaseSecrets) -> Self {
        Self {
            config,
            with_master: false,
            with_replica: false,
            with_prover: false,
            secrets: database_secrets,
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
            secrets: self.secrets,
            with_master: self.with_master,
            with_replica: self.with_replica,
            with_prover: self.with_prover,
        }
    }
}

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

        if self.with_master {
            context.insert_resource(PoolResource::<MasterPool>::new(
                self.secrets.master_url()?,
                self.config.max_connections()?,
                self.config.statement_timeout(),
            ))?;
        }

        if self.with_replica {
            context.insert_resource(PoolResource::<ReplicaPool>::new(
                self.secrets.replica_url()?,
                self.config.max_connections()?,
                self.config.statement_timeout(),
            ))?;
        }

        if self.with_prover {
            context.insert_resource(PoolResource::<ProverPool>::new(
                self.secrets.prover_url()?,
                self.config.max_connections()?,
                self.config.statement_timeout(),
            ))?;
        }

        Ok(())
    }
}
