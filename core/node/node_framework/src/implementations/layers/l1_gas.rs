use std::sync::Arc;

use anyhow::Context;
use zksync_base_token_adjuster::MainNodeBaseTokenAdjuster;
use zksync_config::{
    configs::{chain::StateKeeperConfig, eth_sender::PubdataSendingMode},
    BaseTokenAdjusterConfig, GasAdjusterConfig, GenesisConfig,
};
use zksync_node_fee_model::{l1_gas_price::GasAdjuster, MainNodeFeeInputProvider};
use zksync_types::{fee_model::FeeModelConfig, Address};

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource,
        fee_input::FeeInputResource,
        l1_tx_params::L1TxParamsResource,
        pools::{PoolResource, ReplicaPool},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for sequencer L1 gas interfaces.
/// Adds several resources that depend on L1 gas price.
///
/// ## Requests resources
///
/// - `EthInterfaceResource`
///
/// ## Adds resources
///
/// - `FeeInputResource`
/// - `L1TxParamsResource`
///
/// ## Adds tasks
///
/// - `GasAdjusterTask` (only runs if someone uses the resourced listed above).
#[derive(Debug)]
pub struct SequencerL1GasLayer {
    gas_adjuster_config: GasAdjusterConfig,
    genesis_config: GenesisConfig,
    pubdata_sending_mode: PubdataSendingMode,
    state_keeper_config: StateKeeperConfig,
    base_token_adjuster_config: BaseTokenAdjusterConfig,
    base_token_l1_address: Address,
}

impl SequencerL1GasLayer {
    pub fn new(
        gas_adjuster_config: GasAdjusterConfig,
        genesis_config: GenesisConfig,
        state_keeper_config: StateKeeperConfig,
        pubdata_sending_mode: PubdataSendingMode,
        base_token_adjuster_config: BaseTokenAdjusterConfig,
        base_token_l1_address: Address,
    ) -> Self {
        Self {
            gas_adjuster_config,
            genesis_config,
            pubdata_sending_mode,
            state_keeper_config,
            base_token_adjuster_config,
            base_token_l1_address,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for SequencerL1GasLayer {
    fn layer_name(&self) -> &'static str {
        "sequencer_l1_gas_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let client = context.get_resource::<EthInterfaceResource>().await?.0;
        let adjuster = GasAdjuster::new(
            client,
            self.gas_adjuster_config,
            self.pubdata_sending_mode,
            self.genesis_config.l1_batch_commit_data_generator_mode,
        )
        .await
        .context("GasAdjuster::new()")?;
        let gas_adjuster = Arc::new(adjuster);

        let pool_resource = context.get_resource::<PoolResource<ReplicaPool>>().await?;
        let replica_pool = pool_resource.get().await?;

        let base_token_adjuster = MainNodeBaseTokenAdjuster::new(
            replica_pool.clone(),
            self.base_token_adjuster_config,
            self.base_token_l1_address,
        );

        let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            gas_adjuster.clone(),
            Arc::new(base_token_adjuster),
            FeeModelConfig::from_state_keeper_config(&self.state_keeper_config),
        ));
        context.insert_resource(FeeInputResource(batch_fee_input_provider))?;

        context.insert_resource(L1TxParamsResource(gas_adjuster.clone()))?;

        context.add_task(Box::new(GasAdjusterTask { gas_adjuster }));
        Ok(())
    }
}

#[derive(Debug)]
struct GasAdjusterTask {
    gas_adjuster: Arc<GasAdjuster>,
}

#[async_trait::async_trait]
impl Task for GasAdjusterTask {
    fn id(&self) -> TaskId {
        "gas_adjuster".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Gas adjuster layer is added to provide a resource for anyone to use, but it comes with
        // a support task. If nobody has used the resource, we don't need to run the support task.
        if Arc::strong_count(&self.gas_adjuster) == 1 {
            tracing::info!(
                "Gas adjuster is not used by any other task, not running the support task"
            );
            stop_receiver.0.changed().await?;
            return Ok(());
        }

        self.gas_adjuster.run(stop_receiver.0).await
    }
}
