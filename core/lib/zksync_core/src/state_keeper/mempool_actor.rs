use std::{sync::Arc, time::Duration};

use multivm::vm_latest::utils::fee::derive_base_fee_and_gas_per_pubdata;
use tokio::sync::watch;
use zksync_config::configs::chain::MempoolConfig;
use zksync_dal::ConnectionPool;
use zksync_mempool::L2TxFilter;

use super::{metrics::KEEPER_METRICS, types::MempoolGuard};
use crate::{
    fee_model::{BatchFeeModelInputProvider, MainNodeFeeInputProvider},
    l1_gas_price::L1GasPriceProvider,
};

/// Creates a mempool filter for L2 transactions based on the current L1 gas price.
/// The filter is used to filter out transactions from the mempool that do not cover expenses
/// to process them.
pub fn l2_tx_filter<G: BatchFeeModelInputProvider>(fee_batch_input_provider: &G) -> L2TxFilter {
    let output = fee_batch_input_provider.get_batch_fee_input(false);

    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(output.l1_gas_price, output.fair_l2_gas_price);
    L2TxFilter {
        l1_gas_price: output.l1_gas_price,
        fair_pubdata_price: output.fair_pubdata_price,
        fair_l2_gas_price: output.fair_l2_gas_price,
        fee_per_gas: base_fee,
        gas_per_pubdata: gas_per_pubdata as u32,
    }
}

#[derive(Debug)]
pub struct MempoolFetcher<G> {
    mempool: MempoolGuard,
    batch_fee_info_provider: Arc<G>,
    sync_interval: Duration,
    sync_batch_size: usize,
}

impl<G: BatchFeeModelInputProvider> MempoolFetcher<G> {
    pub fn new(
        mempool: MempoolGuard,
        batch_fee_info_provider: Arc<G>,
        config: &MempoolConfig,
    ) -> Self {
        Self {
            mempool,
            batch_fee_info_provider,
            sync_interval: config.sync_interval(),
            sync_batch_size: config.sync_batch_size,
        }
    }

    pub async fn run(
        mut self,
        pool: ConnectionPool,
        remove_stuck_txs: bool,
        stuck_tx_timeout: Duration,
        fair_l2_gas_price: u64,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        {
            let mut storage = pool.access_storage_tagged("state_keeper").await.unwrap();
            if remove_stuck_txs {
                let removed_txs = storage
                    .transactions_dal()
                    .remove_stuck_txs(stuck_tx_timeout)
                    .await;
                tracing::info!("Number of stuck txs was removed: {}", removed_txs);
            }
            storage.transactions_dal().reset_mempool().await;
        }

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, mempool is shutting down");
                break;
            }
            let latency = KEEPER_METRICS.mempool_sync.start();
            let mut storage = pool.access_storage_tagged("state_keeper").await.unwrap();
            let mempool_info = self.mempool.get_mempool_info();
            let l2_tx_filter = l2_tx_filter(self.batch_fee_info_provider.as_ref());

            let (transactions, nonces) = storage
                .transactions_dal()
                .sync_mempool(
                    mempool_info.stashed_accounts,
                    mempool_info.purged_accounts,
                    l2_tx_filter.gas_per_pubdata,
                    l2_tx_filter.fee_per_gas,
                    self.sync_batch_size,
                )
                .await;
            let all_transactions_loaded = transactions.len() < self.sync_batch_size;
            self.mempool.insert(transactions, nonces);
            latency.observe();
            if all_transactions_loaded {
                tokio::time::sleep(self.sync_interval).await;
            }
        }
        Ok(())
    }
}
