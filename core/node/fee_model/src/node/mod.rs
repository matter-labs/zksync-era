//! Dependency injection for the fee model types.

pub use self::{
    gas_adjuster::GasAdjusterLayer,
    l1_gas::L1GasLayer,
    main_node_fee_params_fetcher::MainNodeFeeParamsFetcherLayer,
    resources::{
        ApiFeeInputResource, BaseTokenRatioProviderResource, GasAdjusterResource,
        SequencerFeeInputResource, TxParamsResource,
    },
};

mod gas_adjuster;
mod l1_gas;
mod main_node_fee_params_fetcher;
mod resources;
