//! This module determines the fees to pay in txs containing blocks submitted to the L1.

pub use gas_adjuster::bounded_gas_adjuster::BoundedGasAdjuster;
pub use gas_adjuster::GasAdjuster;
pub use main_node_fetcher::MainNodeGasPriceFetcher;
pub use singleton::GasAdjusterSingleton;

mod gas_adjuster;
mod main_node_fetcher;
pub mod singleton;

/// Abstraction that provides information about the L1 gas price currently
/// observed by the application.
pub trait L1GasPriceProvider {
    /// Returns a best guess of a realistic value for the L1 gas price.
    /// Return value is in wei.
    fn estimate_effective_gas_price(&self) -> u64;
}

/// Extended version of `L1GasPriceProvider` that can provide parameters
/// to set the fee for an L1 transaction, taking the desired mining time into account.
///
/// This trait, as a bound, should only be used in components that actually sign and send transactions.
pub trait L1TxParamsProvider: L1GasPriceProvider {
    /// Returns the recommended `max_fee_per_gas` value (EIP1559).
    fn get_base_fee(&self, time_in_mempool: u32) -> u64;

    /// Returns the recommended `max_priority_fee_per_gas` value (EIP1559).
    fn get_priority_fee(&self) -> u64;

    /// Returns a lower bound for the `base_fee` value for the next L1 block.
    fn get_next_block_minimal_base_fee(&self) -> u64;
}
