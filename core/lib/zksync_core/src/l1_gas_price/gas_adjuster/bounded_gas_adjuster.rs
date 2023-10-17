use std::{fmt, sync::Arc};

use crate::{l1_gas_price::L1GasPriceProvider, state_keeper::metrics::KEEPER_METRICS};

/// Gas adjuster that bounds the gas price to the specified value.
/// We need this to prevent the gas price from growing too much, because our bootloader is sensitive for the gas price and can fail if it's too high.
/// And for mainnet it's not the case, but for testnet we can have a situation when the gas price is too high.
pub struct BoundedGasAdjuster<G> {
    max_gas_price: u64,
    default_gas_adjuster: Arc<G>,
}

impl<G> fmt::Debug for BoundedGasAdjuster<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundedGasAdjuster")
            .field("max_gas_price", &self.max_gas_price)
            .finish()
    }
}

impl<G> BoundedGasAdjuster<G> {
    pub fn new(max_gas_price: u64, default_gas_adjuster: Arc<G>) -> Self {
        Self {
            max_gas_price,
            default_gas_adjuster,
        }
    }
}

impl<G: L1GasPriceProvider> L1GasPriceProvider for BoundedGasAdjuster<G> {
    fn estimate_effective_gas_price(&self) -> u64 {
        let default_gas_price = self.default_gas_adjuster.estimate_effective_gas_price();
        if default_gas_price > self.max_gas_price {
            tracing::warn!(
                "Effective gas price is too high: {default_gas_price}, using max allowed: {}",
                self.max_gas_price
            );
            KEEPER_METRICS.gas_price_too_high.inc();
            return self.max_gas_price;
        }
        default_gas_price
    }
}
