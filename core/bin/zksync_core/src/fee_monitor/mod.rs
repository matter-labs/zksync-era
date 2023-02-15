//! This module contains utilities for monitoring the fee model performance,
//! i.e. ability of the protocol to cover the costs for its own maintenance.

use std::time::Duration;
use zksync_config::ZkSyncConfig;
use zksync_dal::ConnectionPool;
use zksync_eth_client::clients::http_client::EthereumClient;
use zksync_types::{
    api::BlockId, AccountTreeId, Address, L1BatchNumber, L2_ETH_TOKEN_ADDRESS, U256,
};

/// Component name used to track eth client usage.
const COMPONENT_NAME: &str = "fee-monitor";

/// Inclusive iterator for the (from..=to) blocks range
fn block_range(from: L1BatchNumber, to: L1BatchNumber) -> impl Iterator<Item = L1BatchNumber> {
    (from.0..=to.0).map(L1BatchNumber)
}

/// Helper trait allowing to convert U256 balance representation to the f64
/// with the given amount of decimals.
///
/// Important! Never attempt to use this trait for anything important, because
/// the conversion is, obviously, lossy.
trait BalanceConvert {
    fn to_f64_with_decimals(self, decimals: u8) -> f64;
}

impl BalanceConvert for U256 {
    fn to_f64_with_decimals(self, decimals: u8) -> f64 {
        let divider = U256::from(10u64.pow(decimals as u32));
        let (quotient, remainder) = self.div_mod(divider);
        let remainder_fractional = (remainder.as_u128() as f64) * 10.0f64.powf(-(decimals as f64));

        quotient.as_u128() as f64 + remainder_fractional
    }
}

#[derive(Debug)]
pub struct FeeMonitor {
    operator_address: Address,
    fee_account_address: Address,

    storage: ConnectionPool,
    client: EthereumClient,

    next_finalized_block: L1BatchNumber,
}

impl FeeMonitor {
    pub async fn new(
        config: &ZkSyncConfig,
        storage: ConnectionPool,
        client: EthereumClient,
    ) -> Self {
        let mut storage_processor = storage.access_storage().await;
        let latest_l1_batch_finalized = storage_processor
            .blocks_dal()
            .get_number_of_last_block_executed_on_eth()
            .unwrap_or_default();
        drop(storage_processor);

        Self {
            operator_address: config.eth_sender.sender.operator_commit_eth_addr,
            fee_account_address: config.chain.state_keeper.fee_account_addr,

            storage,
            client,

            next_finalized_block: latest_l1_batch_finalized.next(),
        }
    }

    pub async fn run(mut self) {
        // We don't need these metrics to be reported often.
        let mut timer = tokio::time::interval(Duration::from_secs(15));

        loop {
            timer.tick().await;
            self.run_iter().await;
        }
    }

    async fn run_iter(&mut self) {
        let last_finalized = {
            let mut storage = self.storage.access_storage().await;
            storage
                .blocks_dal()
                .get_number_of_last_block_executed_on_eth()
                .unwrap_or_default()
        };

        let _ = self.report_balances().await.map_err(|err| {
            vlog::warn!("Unable to report account balances in fee monitor: {err}");
        });

        // Only report data if new blocks were finalized.
        if last_finalized >= self.next_finalized_block {
            let _ = self
                .report_collected_fees(last_finalized)
                .await
                .map_err(|err| {
                    vlog::warn!("Unable to report collected fees in fee monitor: {err}");
                });
            let _ = self
                .report_l1_batch_finalized(last_finalized)
                .await
                .map_err(|err| {
                    vlog::warn!("Unable to report l1 batch finalization in fee monitor: {err}");
                });

            self.next_finalized_block = last_finalized.next();
        }
    }

    async fn report_balances(&self) -> anyhow::Result<()> {
        let mut storage = self.storage.access_storage().await;
        let mut operator_balance_l1 = self
            .client
            .eth_balance(self.operator_address, COMPONENT_NAME)
            .await?
            .to_f64_with_decimals(18);
        let mut fee_account_balance_l1 = self
            .client
            .eth_balance(self.fee_account_address, COMPONENT_NAME)
            .await?
            .to_f64_with_decimals(18);
        let mut fee_account_balance_l2 = storage
            .storage_web3_dal()
            .standard_token_historical_balance(
                AccountTreeId::new(L2_ETH_TOKEN_ADDRESS),
                AccountTreeId::new(self.fee_account_address),
                BlockId::Number(zksync_types::api::BlockNumber::Pending),
            )??
            .to_f64_with_decimals(18);

        // Limit balances to sane values to render them adequatily on the localhost.
        for balance in [
            &mut operator_balance_l1,
            &mut fee_account_balance_l1,
            &mut fee_account_balance_l2,
        ] {
            // We're unlikely to keep more than 1000 ETH on hot wallets in any real environment.
            const MAX_BALANCE_TO_DISPLAY_ETH: f64 = 1000.0f64;
            *balance = balance.min(MAX_BALANCE_TO_DISPLAY_ETH);
        }

        metrics::gauge!("fee_monitor.balances", operator_balance_l1, "account" => "operator_l1");
        metrics::gauge!("fee_monitor.balances", fee_account_balance_l1, "account" => "fee_account_l1");
        metrics::gauge!("fee_monitor.balances", fee_account_balance_l2, "account" => "fee_account_l2");

        Ok(())
    }

    async fn report_collected_fees(&mut self, last_finalized: L1BatchNumber) -> anyhow::Result<()> {
        let mut storage = self.storage.access_storage().await;
        for block_number in block_range(self.next_finalized_block, last_finalized) {
            let collected_fees = storage
                .fee_monitor_dal()
                .fetch_erc20_transfers(block_number, self.fee_account_address)?;

            let total_fee_wei: U256 = collected_fees
                .into_iter()
                .fold(U256::zero(), |acc, x| acc + x);

            // Convert value to gwei to reduce verbosity.
            let fee_in_gwei = total_fee_wei.to_f64_with_decimals(9);
            metrics::gauge!("fee_monitor.collected_fees", fee_in_gwei);
            vlog::info!("Collected fees in block {block_number}: {fee_in_gwei:.6} gwei");
        }

        Ok(())
    }

    async fn report_l1_batch_finalized(
        &mut self,
        last_finalized: L1BatchNumber,
    ) -> anyhow::Result<()> {
        let mut storage = self.storage.access_storage().await;
        for block_number in block_range(self.next_finalized_block, last_finalized) {
            let block_data = storage
                .fee_monitor_dal()
                .get_block_gas_consumption(block_number)?;
            let total_wei_spent = U256::from(block_data.wei_spent());

            // Convert value to gwei to reduce verbosity.
            let gwei_spent = total_wei_spent.to_f64_with_decimals(9);
            metrics::gauge!("fee_monitor.expenses", gwei_spent);
            vlog::info!("Block processing expenses in block {block_number}: {gwei_spent:.6} gwei");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(val: u64, expected: f64, decimals: u8) {
        // That's a bad way to compare floats, mmkay?
        // However we aren't going for precision anyways, so don't tell anyone. ( ͡° ͜ʖ ͡°)
        let result = U256::from(val).to_f64_with_decimals(decimals);
        let abs_diff = (result - expected).abs();
        assert!(
            abs_diff < 0.000001f64,
            "Value mismatch: expected {}, got {}",
            expected,
            result
        );
    }

    #[test]
    fn to_f64_with_decimals() {
        check(1000000, 1.0, 6);
        check(1000001, 1.000001, 6);
        check(1800001, 1.800001, 6);
        check(3241500000000000000, 3.2415, 18);
    }
}
