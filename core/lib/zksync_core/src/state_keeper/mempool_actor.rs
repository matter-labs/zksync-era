use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context as _;
use multivm::utils::derive_base_fee_and_gas_per_pubdata;
use tokio::sync::watch;
use zksync_config::configs::chain::MempoolConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_mempool::L2TxFilter;
use zksync_types::{get_nonce_key, Address, Nonce, Transaction, VmVersion};

use super::{metrics::KEEPER_METRICS, types::MempoolGuard};
use crate::{api_server::execution_sandbox::BlockArgs, fee_model::BatchFeeModelInputProvider};

/// Creates a mempool filter for L2 transactions based on the current L1 gas price.
/// The filter is used to filter out transactions from the mempool that do not cover expenses
/// to process them.
pub async fn l2_tx_filter(
    batch_fee_input_provider: &dyn BatchFeeModelInputProvider,
    vm_version: VmVersion,
) -> L2TxFilter {
    let fee_input = batch_fee_input_provider.get_batch_fee_input().await;

    let (base_fee, gas_per_pubdata) = derive_base_fee_and_gas_per_pubdata(fee_input, vm_version);
    L2TxFilter {
        fee_input,
        fee_per_gas: base_fee,
        gas_per_pubdata: gas_per_pubdata as u32,
    }
}

#[derive(Debug)]
pub struct MempoolFetcher<G> {
    mempool: MempoolGuard,
    batch_fee_input_provider: Arc<G>,
    sync_interval: Duration,
    sync_batch_size: usize,
}

impl<G: BatchFeeModelInputProvider> MempoolFetcher<G> {
    pub fn new(
        mempool: MempoolGuard,
        batch_fee_input_provider: Arc<G>,
        config: &MempoolConfig,
    ) -> Self {
        Self {
            mempool,
            batch_fee_input_provider,
            sync_interval: config.sync_interval(),
            sync_batch_size: config.sync_batch_size,
        }
    }

    pub async fn run(
        mut self,
        pool: ConnectionPool,
        remove_stuck_txs: bool,
        stuck_tx_timeout: Duration,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        {
            let mut storage = pool.access_storage_tagged("state_keeper").await?;
            if remove_stuck_txs {
                let removed_txs = storage
                    .transactions_dal()
                    .remove_stuck_txs(stuck_tx_timeout)
                    .await
                    .context("failed removing stuck transactions")?;
                tracing::info!("Number of stuck txs was removed: {removed_txs}");
            }
            storage
                .transactions_dal()
                .reset_mempool()
                .await
                .context("failed resetting mempool")?;
        }

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, mempool is shutting down");
                break;
            }
            let latency = KEEPER_METRICS.mempool_sync.start();
            let mut storage = pool.access_storage_tagged("state_keeper").await?;
            let mempool_info = self.mempool.get_mempool_info();

            let latest_miniblock = BlockArgs::pending(&mut storage)
                .await
                .context("failed obtaining latest miniblock")?;
            let protocol_version = latest_miniblock
                .resolve_block_info(&mut storage)
                .await
                .with_context(|| format!("failed resolving block info for {latest_miniblock:?}"))?
                .protocol_version;

            let l2_tx_filter = l2_tx_filter(
                self.batch_fee_input_provider.as_ref(),
                protocol_version.into(),
            )
            .await;

            let transactions = storage
                .transactions_dal()
                .sync_mempool(
                    &mempool_info.stashed_accounts,
                    &mempool_info.purged_accounts,
                    l2_tx_filter.gas_per_pubdata,
                    l2_tx_filter.fee_per_gas,
                    self.sync_batch_size,
                )
                .await
                .context("failed syncing mempool")?;
            let nonces = Self::get_transaction_nonces(&mut storage, &transactions).await?;
            drop(storage);

            let all_transactions_loaded = transactions.len() < self.sync_batch_size;
            self.mempool.insert(transactions, nonces);
            latency.observe();
            if all_transactions_loaded {
                tokio::time::sleep(self.sync_interval).await;
            }
        }
        Ok(())
    }

    /// Loads nonces for all distinct `transactions` initiators from the storage.
    async fn get_transaction_nonces(
        storage: &mut StorageProcessor<'_>,
        transactions: &[Transaction],
    ) -> anyhow::Result<HashMap<Address, Nonce>> {
        let (nonce_keys, address_by_nonce_key): (Vec<_>, HashMap<_, _>) = transactions
            .iter()
            .map(|tx| {
                let address = tx.initiator_account();
                let nonce_key = get_nonce_key(&address).hashed_key();
                (nonce_key, (nonce_key, address))
            })
            .unzip();

        let nonce_values = storage
            .storage_web3_dal()
            .get_values(&nonce_keys)
            .await
            .context("failed getting nonces from storage")?;

        Ok(nonce_values
            .into_iter()
            .map(|(nonce_key, nonce_value)| {
                let nonce = Nonce(zksync_utils::h256_to_u32(nonce_value));
                (address_by_nonce_key[&nonce_key], nonce)
            })
            .collect())
    }
}
