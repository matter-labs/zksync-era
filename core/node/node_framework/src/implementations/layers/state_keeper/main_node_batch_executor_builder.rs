use zksync_config::{configs::chain::StateKeeperConfig, DBConfig};
use zksync_core::state_keeper::MainBatchExecutorBuilder;

use crate::{
    implementations::resources::{
        pools::MasterPoolResource, state_keeper::L1BatchExecutorBuilderResource,
    },
    resource::Unique,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct MainNodeBatchExecutorBuilderLayer {
    db_config: DBConfig,
    state_keeper_config: StateKeeperConfig,
}

impl MainNodeBatchExecutorBuilderLayer {
    pub fn new(db_config: DBConfig, state_keeper_config: StateKeeperConfig) -> Self {
        Self {
            db_config,
            state_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeBatchExecutorBuilderLayer {
    fn layer_name(&self) -> &'static str {
        "main_node_batch_executor_builder_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool = context.get_resource::<MasterPoolResource>().await?;

        let builder = MainBatchExecutorBuilder::new(
            self.db_config.state_keeper_db_path,
            master_pool.get_singleton().await?,
            self.state_keeper_config.max_allowed_l2_tx_gas_limit.into(),
            self.state_keeper_config.save_call_traces,
            self.state_keeper_config.upload_witness_inputs_to_gcs,
            self.state_keeper_config.enum_index_migration_chunk_size(),
            false,
        );

        context.add_resource(L1BatchExecutorBuilderResource(Unique::new(Box::new(
            builder,
        ))))?;
        Ok(())
    }
}
