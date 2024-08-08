use std::sync::Arc;

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_node_fee_model::MainNodeFeeInputProvider;
use zksync_types::fee_model::FeeModelConfig;

use crate::{
    implementations::resources::{
        base_token_ratio_provider::BaseTokenRatioProviderResource,
        fee_input::SequencerFeeInputResource, gas_adjuster::GasAdjusterResource,
        l1_tx_params::L1TxParamsResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for sequencer L1 gas interfaces.
/// Adds several resources that depend on L1 gas price.
#[derive(Debug)]
pub struct SequencerL1GasLayer {
    state_keeper_config: StateKeeperConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub gas_adjuster: GasAdjusterResource,
    /// If not provided, the base token assumed to be ETH, and the ratio will be constant.
    #[context(default)]
    pub base_token_ratio_provider: BaseTokenRatioProviderResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub fee_input: SequencerFeeInputResource,
    pub l1_tx_params: L1TxParamsResource,
}

impl SequencerL1GasLayer {
    pub fn new(state_keeper_config: StateKeeperConfig) -> Self {
        Self {
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
        let ratio_provider = input.base_token_ratio_provider;

        let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            input.gas_adjuster.0.clone(),
            ratio_provider.0,
            FeeModelConfig::from_state_keeper_config(&self.state_keeper_config),
        ));
        Ok(Output {
            fee_input: batch_fee_input_provider.into(),
            l1_tx_params: input.gas_adjuster.0.into(),
        })
    }
}
