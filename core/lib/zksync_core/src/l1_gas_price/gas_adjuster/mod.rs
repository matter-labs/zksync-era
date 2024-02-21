//! This module determines the fees to pay in txs containing blocks submitted to the L1.

use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use tokio::sync::watch;
use zksync_config::GasAdjusterConfig;
use zksync_eth_client::{Error, EthInterface};

use self::metrics::METRICS;
use super::L1TxParamsProvider;
use crate::state_keeper::metrics::KEEPER_METRICS;

mod metrics;
#[cfg(test)]
mod tests;

/// This component keeps track of the median base_fee from the last `max_base_fee_samples` blocks.
/// It is used to adjust the base_fee of transactions sent to L1.
#[derive(Debug)]
pub struct GasAdjuster {
    pub(super) statistics: GasStatistics,
    pub(super) config: GasAdjusterConfig,
    eth_client: Arc<dyn EthInterface>,
}

impl GasAdjuster {
    pub async fn new(
        eth_client: Arc<dyn EthInterface>,
        config: GasAdjusterConfig,
    ) -> Result<Self, Error> {
        // Subtracting 1 from the "latest" block number to prevent errors in case
        // the info about the latest block is not yet present on the node.
        // This sometimes happens on Infura.
        let current_block = eth_client
            .block_number("gas_adjuster")
            .await?
            .as_usize()
            .saturating_sub(1);
        let history = eth_client
            .base_fee_history(current_block, config.max_base_fee_samples, "gas_adjuster")
            .await?;
        Ok(Self {
            statistics: GasStatistics::new(config.max_base_fee_samples, current_block, &history),
            eth_client,
            config,
        })
    }

    /// Performs an actualization routine for `GasAdjuster`.
    /// This method is intended to be invoked periodically.
    pub async fn keep_updated(&self) -> Result<(), Error> {
        // Subtracting 1 from the "latest" block number to prevent errors in case
        // the info about the latest block is not yet present on the node.
        // This sometimes happens on Infura.
        let current_block = self
            .eth_client
            .block_number("gas_adjuster")
            .await?
            .as_usize()
            .saturating_sub(1);

        let last_processed_block = self.statistics.last_processed_block();

        if current_block > last_processed_block {
            // Report the current price to be gathered by the statistics module.
            let history = self
                .eth_client
                .base_fee_history(
                    current_block,
                    current_block - last_processed_block,
                    "gas_adjuster",
                )
                .await?;

            METRICS
                .current_base_fee_per_gas
                .set(*history.last().unwrap());
            self.statistics.add_samples(&history);
        }
        Ok(())
    }

    fn bound_gas_price(&self, gas_price: u64) -> u64 {
        let max_l1_gas_price = self.config.max_l1_gas_price();
        if gas_price > max_l1_gas_price {
            tracing::warn!(
                "Effective gas price is too high: {gas_price}, using max allowed: {}",
                max_l1_gas_price
            );
            KEEPER_METRICS.gas_price_too_high.inc();
            return max_l1_gas_price;
        }
        gas_price
    }

    pub async fn run(self: Arc<Self>, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, gas_adjuster is shutting down");
                break;
            }

            if let Err(err) = self.keep_updated().await {
                tracing::warn!("Cannot add the base fee to gas statistics: {}", err);
            }

            tokio::time::sleep(self.config.poll_period()).await;
        }
        Ok(())
    }

    fn into_l1_gas_price_provider(self: Arc<Self>) -> Arc<dyn L1GasPriceProvider>;

    fn into_l1_tx_params_provider(self: Arc<Self>) -> Arc<dyn L1TxParamsProvider>;

    fn config(&self) -> &GasAdjusterConfig;

    fn eth_client(&self) -> &E;

    fn statistics(&self) -> &GasStatistics;
}

/// Helper structure responsible for collecting the data about recent transactions,
/// calculating the median base fee.
#[derive(Debug, Clone, Default)]
pub(super) struct GasStatisticsInner {
    samples: VecDeque<u64>,
    median_cached: u64,
    max_samples: usize,
    last_processed_block: usize,
}

impl GasStatisticsInner {
    fn new(max_samples: usize, block: usize, fee_history: &[u64]) -> Self {
        let mut statistics = Self {
            max_samples,
            samples: VecDeque::with_capacity(max_samples),
            median_cached: 0,
            last_processed_block: 0,
        };

        statistics.add_samples(fee_history);

        Self {
            last_processed_block: block,
            ..statistics
        }
    }

    fn median(&self) -> u64 {
        self.median_cached
    }

    fn last_added_value(&self) -> u64 {
        self.samples.back().copied().unwrap_or(self.median_cached)
    }

    fn add_samples(&mut self, fees: &[u64]) {
        self.samples.extend(fees);
        self.last_processed_block += fees.len();

        let extra = self.samples.len().saturating_sub(self.max_samples);
        self.samples.drain(..extra);

        let mut samples: Vec<_> = self.samples.iter().cloned().collect();
        let (_, &mut median, _) = samples.select_nth_unstable(self.samples.len() / 2);

        self.median_cached = median;
    }
}

#[derive(Debug, Default)]
pub struct GasStatistics(RwLock<GasStatisticsInner>);

impl GasStatistics {
    pub fn new(max_samples: usize, block: usize, fee_history: &[u64]) -> Self {
        Self(RwLock::new(GasStatisticsInner::new(
            max_samples,
            block,
            fee_history,
        )))
    }

    pub fn median(&self) -> u64 {
        self.0.read().unwrap().median()
    }

    pub fn last_added_value(&self) -> u64 {
        self.0.read().unwrap().last_added_value()
    }

    pub fn add_samples(&self, fees: &[u64]) {
        self.0.write().unwrap().add_samples(fees)
    }

    pub fn last_processed_block(&self) -> usize {
        self.0.read().unwrap().last_processed_block
    }
}
