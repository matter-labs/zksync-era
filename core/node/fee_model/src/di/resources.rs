use std::sync::Arc;

use zksync_node_framework::resource::Resource;

use crate::{
    l1_gas_price::{GasAdjuster, TxParamsProvider},
    BaseTokenRatioProvider, BatchFeeModelInputProvider, NoOpRatioProvider,
};

/// A resource that provides [`BaseTokenRatioProvider`] implementation to the service.
#[derive(Debug, Clone)]
pub struct BaseTokenRatioProviderResource(pub Arc<dyn BaseTokenRatioProvider>);

impl Default for BaseTokenRatioProviderResource {
    fn default() -> Self {
        Self(Arc::new(NoOpRatioProvider::default()))
    }
}

impl Resource for BaseTokenRatioProviderResource {
    fn name() -> String {
        "common/base_token_ratio_provider".into()
    }
}

impl<T: BaseTokenRatioProvider> From<Arc<T>> for BaseTokenRatioProviderResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
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

/// A resource that provides [`GasAdjuster`] to the service.
#[derive(Debug, Clone)]
pub struct GasAdjusterResource(pub Arc<GasAdjuster>);

impl Resource for GasAdjusterResource {
    fn name() -> String {
        "common/gas_adjuster".into()
    }
}

impl From<Arc<GasAdjuster>> for GasAdjusterResource {
    fn from(gas_adjuster: Arc<GasAdjuster>) -> Self {
        Self(gas_adjuster)
    }
}

/// A resource that provides [`TxParamsProvider`] implementation to the service.
#[derive(Debug, Clone)]
pub struct TxParamsResource(pub Arc<dyn TxParamsProvider>);

impl Resource for TxParamsResource {
    fn name() -> String {
        "common/tx_params".into()
    }
}

impl<T: TxParamsProvider> From<Arc<T>> for TxParamsResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}
