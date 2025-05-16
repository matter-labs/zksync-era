use std::sync::Arc;

use zksync_node_framework::resource::Resource;

use crate::{
    l1_gas_price::{GasAdjuster, TxParamsProvider},
    BaseTokenRatioProvider, BatchFeeModelInputProvider,
};

impl Resource for dyn BaseTokenRatioProvider {
    fn name() -> String {
        "common/base_token_ratio_provider".into()
    }
}

/// A resource that provides [`BatchFeeModelInputProvider`] implementation to the service and is used by sequencer.
#[derive(Debug, Clone)]
pub struct SequencerFeeInputResource(pub Arc<dyn BatchFeeModelInputProvider>);

impl Resource for SequencerFeeInputResource {
    fn name() -> String {
        "common/sequencer_fee_input".into()
    }
}

impl<T: BatchFeeModelInputProvider> From<Arc<T>> for SequencerFeeInputResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}

/// A resource that provides [`BatchFeeModelInputProvider`] implementation to the service  and is used by API.
#[derive(Debug, Clone)]
pub struct ApiFeeInputResource(pub Arc<dyn BatchFeeModelInputProvider>);

impl Resource for ApiFeeInputResource {
    fn name() -> String {
        "common/api_fee_input".into()
    }
}

impl<T: BatchFeeModelInputProvider> From<Arc<T>> for ApiFeeInputResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}

impl Resource for GasAdjuster {
    fn name() -> String {
        "common/gas_adjuster".into()
    }
}

impl Resource for dyn TxParamsProvider {
    fn name() -> String {
        "common/tx_params".into()
    }
}
