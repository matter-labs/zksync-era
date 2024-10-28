use std::sync::Arc;

use zksync_config::configs::chain::{FeeModelVersion, StateKeeperConfig};
use zksync_node_fee_model::{ApiFeeInputProvider, MainNodeFeeInputProvider};
use zksync_types::fee_model::{FeeModelConfig, FeeModelConfigV1, FeeModelConfigV2};

use crate::{
    implementations::resources::{
        base_token_ratio_provider::BaseTokenRatioProviderResource,
        fee_input::{ApiFeeInputResource, SequencerFeeInputResource},
        gas_adjuster::GasAdjusterResource,
        l1_tx_params::TxParamsResource,
        pools::{PoolResource, ReplicaPool},
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for L1 gas interfaces.
/// Adds several resources that depend on L1 gas price.
#[derive(Debug)]
pub struct L1GasLayer {
    fee_model_config: FeeModelConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub gas_adjuster: GasAdjusterResource,
    pub replica_pool: PoolResource<ReplicaPool>,
    /// If not provided, the base token assumed to be ETH, and the ratio will be constant.
    #[context(default)]
    pub base_token_ratio_provider: BaseTokenRatioProviderResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub sequencer_fee_input: SequencerFeeInputResource,
    pub api_fee_input: ApiFeeInputResource,
    pub l1_tx_params: TxParamsResource,
}

impl L1GasLayer {
    pub fn new(state_keeper_config: &StateKeeperConfig) -> Self {
        Self {
            fee_model_config: Self::map_config(state_keeper_config),
        }
    }

    fn map_config(state_keeper_config: &StateKeeperConfig) -> FeeModelConfig {
        match state_keeper_config.fee_model_version {
            FeeModelVersion::V1 => FeeModelConfig::V1(FeeModelConfigV1 {
                minimal_l2_gas_price: state_keeper_config.minimal_l2_gas_price,
            }),
            FeeModelVersion::V2 => FeeModelConfig::V2(FeeModelConfigV2 {
                minimal_l2_gas_price: state_keeper_config.minimal_l2_gas_price,
                compute_overhead_part: state_keeper_config.compute_overhead_part,
                pubdata_overhead_part: state_keeper_config.pubdata_overhead_part,
                batch_overhead_l1_gas: state_keeper_config.batch_overhead_l1_gas,
                max_gas_per_batch: state_keeper_config.max_gas_per_batch,
                max_pubdata_per_batch: state_keeper_config.max_pubdata_per_batch,
            }),
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for L1GasLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "l1_gas_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let ratio_provider = input.base_token_ratio_provider;

        let main_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            input.gas_adjuster.0.clone(),
            ratio_provider.0,
            self.fee_model_config,
        ));

        let replica_pool = input.replica_pool.get().await?;
        let api_fee_input_provider = Arc::new(ApiFeeInputProvider::new(
            main_fee_input_provider.clone(),
            replica_pool,
        ));

        Ok(Output {
            sequencer_fee_input: main_fee_input_provider.into(),
            api_fee_input: api_fee_input_provider.into(),
            l1_tx_params: input.gas_adjuster.0.into(),
        })
    }
}
