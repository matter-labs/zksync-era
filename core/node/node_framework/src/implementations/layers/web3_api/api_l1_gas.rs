use std::sync::Arc;

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_node_fee_model::{ApiFeeInputProvider, MainNodeFeeInputProvider};
use zksync_types::fee_model::FeeModelConfig;

use crate::{
    implementations::resources::{
        base_token_ratio_provider::BaseTokenRatioProviderResource,
        fee_input::ApiFeeInputResource,
        gas_adjuster::GasAdjusterResource,
        pools::{PoolResource, ReplicaPool},
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for API L1 gas interfaces.
/// Adds several resources that depend on L1 gas price.
#[derive(Debug)]
pub struct ApiL1GasLayer {
    state_keeper_config: StateKeeperConfig,
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
    pub fee_input: ApiFeeInputResource,
}

impl ApiL1GasLayer {
    pub fn new(state_keeper_config: StateKeeperConfig) -> Self {
        Self {
            state_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ApiL1GasLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "api_l1_gas_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let ratio_provider = input.base_token_ratio_provider;

        let main_node_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            input.gas_adjuster.0.clone(),
            ratio_provider.0,
            FeeModelConfig::from_state_keeper_config(&self.state_keeper_config),
        ));

        let replica_pool = input.replica_pool.get().await?;
        let api_fee_input_provider = Arc::new(ApiFeeInputProvider::new(
            main_node_fee_input_provider,
            replica_pool,
        ));

        Ok(Output {
            fee_input: api_fee_input_provider.into(),
        })
    }
}
