//! This module determines the fees to pay in txs containing blocks submitted to the L1.

use std::fmt;

pub use self::{
    gas_adjuster::{GasAdjuster, GasAdjusterClient},
    main_node_fetcher::MainNodeFeeParamsFetcher,
};

mod blob_base_fee_predictor;
mod gas_adjuster;
mod main_node_fetcher;

/// Abstraction that provides parameters to set the fee for an L1 transaction, taking the desired
/// mining time into account.
///
/// This trait, as a bound, should only be used in components that actually sign and send transactions.
pub trait TxParamsProvider: fmt::Debug + 'static + Send + Sync {
    /// Returns the recommended `max_fee_per_gas` value (EIP1559).
    fn get_base_fee(&self, time_in_mempool_in_l1_blocks: u32) -> u64;

    /// Returns the recommended `max_fee_per_gas` value if using gateway.
    fn gateway_get_base_fee(&self, time_in_mempool_in_l1_blocks: u32) -> u64;

    /// Returns the recommended `max_priority_fee_per_gas` value (EIP1559).
    fn get_priority_fee(&self) -> u64;

    /// Returns a lower bound for the `base_fee` value for the next L1 block.
    fn get_next_block_minimal_base_fee(&self) -> u64;

    /// Returns a lower bound for the `base_fee` value for the next L1 block.
    fn get_next_block_minimal_blob_base_fee(&self) -> u64;

    /// Returns the recommended `max_fee_per_gas` value (EIP1559) for blob transaction.
    fn get_blob_tx_base_fee(&self, time_in_mempool_in_l1_blocks: u32) -> u64;

    /// Returns the recommended `max_blob_fee_per_gas` value (EIP4844) for blob transaction.
    fn get_blob_tx_blob_base_fee(&self, time_in_mempool_in_l1_blocks: u32) -> u64;

    /// Returns the recommended `max_priority_fee_per_gas` value (EIP1559) for blob transaction.
    fn get_blob_tx_priority_fee(&self) -> u64;

    /// Returns the recommended `max_price_per_pubdata` value for gateway transactions.
    fn get_gateway_price_per_pubdata(&self, time_in_mempool_in_l1_blocks: u32) -> u64;

    /// Returns the recommended `l2_pubdata_price` value for gateway transactions.
    fn get_gateway_l2_pubdata_price(&self, time_in_mempool_in_l1_blocks: u32) -> u64;

    /// Returns `b` parameter of the pricing formula.
    fn get_parameter_b(&self) -> f64;
}
