use std::sync::Arc;

use anyhow::Context;
use zksync_base_token_adjuster::{BaseTokenAdjuster, MainNodeBaseTokenAdjuster};
use zksync_config::{
    configs::{chain::StateKeeperConfig, eth_sender::PubdataSendingMode},
    BaseTokenAdjusterConfig, ContractsConfig, GasAdjusterConfig, GenesisConfig,
};
use zksync_node_fee_model::{l1_gas_price::GasAdjuster, MainNodeFeeInputProvider};
use zksync_types::{fee_model::FeeModelConfig, Address, L1_ETH_CONTRACT_ADDRESS};

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

#[derive(Debug)]
pub struct SequencerL1GasLayer {
    gas_adjuster_config: GasAdjusterConfig,
    genesis_config: GenesisConfig,
    pubdata_sending_mode: PubdataSendingMode,
    state_keeper_config: StateKeeperConfig,
    base_token_adjuster_config: BaseTokenAdjusterConfig,
    contracts_config: ContractsConfig,
}

impl SequencerL1GasLayer {
    pub fn new(
        gas_adjuster_config: GasAdjusterConfig,
        genesis_config: GenesisConfig,
        state_keeper_config: StateKeeperConfig,
        pubdata_sending_mode: PubdataSendingMode,
        base_token_adjuster_config: BaseTokenAdjusterConfig,
        contracts_config: ContractsConfig,
    ) -> Self {
        Self {
            gas_adjuster_config,
            genesis_config,
            pubdata_sending_mode,
            state_keeper_config,
            base_token_adjuster_config,
            contracts_config,
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

        let mut base_token_adjuster: Option<Arc<dyn BaseTokenAdjuster>> = None;

        // check if the base token is ETH, if not, we need to use the real adjuster
        // otherwise, this is not needed
        if let Some(base_token_addr) = self.contracts_config.base_token_addr {
            if base_token_addr != L1_ETH_CONTRACT_ADDRESS {
                let main_node_base_token_adjuster = MainNodeBaseTokenAdjuster::new(
                    replica_pool.clone(),
                    self.base_token_adjuster_config,
                );
                base_token_adjuster = Some(Arc::new(main_node_base_token_adjuster))
            }
        }

        let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            gas_adjuster.clone(),
            base_token_adjuster,
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

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.gas_adjuster.run(stop_receiver.0).await
    }
}
