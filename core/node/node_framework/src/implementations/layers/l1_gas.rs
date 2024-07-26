use std::sync::Arc;

use anyhow::Context;
use zksync_config::{
    configs::{chain::StateKeeperConfig, eth_sender::PubdataSendingMode},
    GasAdjusterConfig, GenesisConfig,
};
use zksync_node_fee_model::{l1_gas_price::GasAdjuster, MainNodeFeeInputProvider};
use zksync_types::fee_model::FeeModelConfig;

use crate::{
    implementations::resources::{
        base_token_ratio_provider::BaseTokenRatioProviderResource,
        eth_interface::EthInterfaceResource, fee_input::FeeInputResource,
        l1_tx_params::L1TxParamsResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for sequencer L1 gas interfaces.
/// Adds several resources that depend on L1 gas price.
#[derive(Debug)]
pub struct SequencerL1GasLayer {
    gas_adjuster_config: GasAdjusterConfig,
    genesis_config: GenesisConfig,
    pubdata_sending_mode: PubdataSendingMode,
    state_keeper_config: StateKeeperConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
    /// If not provided, the base token assumed to be ETH, and the ratio will be constant.
    #[context(default)]
    pub base_token_ratio_provider: BaseTokenRatioProviderResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub fee_input: FeeInputResource,
    pub l1_tx_params: L1TxParamsResource,
    /// Only runs if someone uses the resources listed above.
    #[context(task)]
    pub gas_adjuster_task: GasAdjusterTask,
}

impl SequencerL1GasLayer {
    pub fn new(
        gas_adjuster_config: GasAdjusterConfig,
        genesis_config: GenesisConfig,
        state_keeper_config: StateKeeperConfig,
        pubdata_sending_mode: PubdataSendingMode,
    ) -> Self {
        Self {
            gas_adjuster_config,
            genesis_config,
            pubdata_sending_mode,
            state_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for SequencerL1GasLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "sequencer_l1_gas_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let client = input.eth_client.0;
        let adjuster = GasAdjuster::new(
            client,
            self.gas_adjuster_config,
            self.pubdata_sending_mode,
            self.genesis_config.l1_batch_commit_data_generator_mode,
        )
        .await
        .context("GasAdjuster::new()")?;
        let gas_adjuster = Arc::new(adjuster);

        let ratio_provider = input.base_token_ratio_provider;

        let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            gas_adjuster.clone(),
            ratio_provider.0.clone(),
            FeeModelConfig::from_state_keeper_config(&self.state_keeper_config),
        ));
        Ok(Output {
            fee_input: batch_fee_input_provider.into(),
            l1_tx_params: gas_adjuster.clone().into(),
            gas_adjuster_task: GasAdjusterTask { gas_adjuster },
        })
    }
}

#[derive(Debug)]
pub struct GasAdjusterTask {
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
