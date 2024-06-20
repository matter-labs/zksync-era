use std::{collections::HashMap, ops, sync::Arc, time::Duration};

use async_trait::async_trait;
use rand::{prelude::SliceRandom, Rng};
use tokio::sync::RwLock;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_node_test_utils::{
    create_l1_batch_metadata, create_l2_block, execute_l2_transaction,
    l1_batch_metadata_to_commitment_artifacts,
};
use zksync_state_keeper::{StateKeeperOutputHandler, UpdatesManager};
use zksync_test_account::Account;
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader, L2BlockHasher},
    fee::{Fee, TransactionExecutionMetrics},
    get_intrinsic_constants,
    l2::L2Tx,
    utils::storage_key_for_standard_token_balance,
    AccountTreeId, Address, Execute, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey,
    StorageLog, StorageLogKind, StorageValue, H160, H256, L2_BASE_TOKEN_ADDRESS, U256,
};
use zksync_utils::u256_to_h256;

use super::{OutputHandlerFactory, VmRunnerIo};

mod output_handler;
mod process;
mod storage;

#[derive(Debug, Default)]
struct IoMock {
    current: L1BatchNumber,
    max: u32,
}

#[async_trait]
impl VmRunnerIo for Arc<RwLock<IoMock>> {
    fn name(&self) -> &'static str {
        "io_mock"
    }

    async fn latest_processed_batch(
        &self,
        _conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(self.read().await.current)
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        _conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        let io = self.read().await;
        Ok(io.current + io.max)
    }

    async fn mark_l1_batch_as_completed(
        &self,
        _conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        self.write().await.current = l1_batch_number;
        Ok(())
    }
}

mod wait {
    use std::{sync::Arc, time::Duration};

    use backon::{ConstantBuilder, Retryable};
    use tokio::sync::RwLock;
    use zksync_types::L1BatchNumber;

    use crate::tests::IoMock;

    pub(super) async fn for_batch(
        io: Arc<RwLock<IoMock>>,
        l1_batch_number: L1BatchNumber,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        const RETRY_INTERVAL: Duration = Duration::from_millis(500);

        let max_tries = (timeout.as_secs_f64() / RETRY_INTERVAL.as_secs_f64()).ceil() as u64;
        (|| async {
            let current = io.read().await.current;
            anyhow::ensure!(
                current == l1_batch_number,
                "Batch #{} has not been processed yet (current is #{})",
                l1_batch_number,
                current
            );
            Ok(())
        })
        .retry(
            &ConstantBuilder::default()
                .with_delay(RETRY_INTERVAL)
                .with_max_times(max_tries as usize),
        )
        .await
    }

    pub(super) async fn for_batch_progressively(
        io: Arc<RwLock<IoMock>>,
        l1_batch_number: L1BatchNumber,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        const SLEEP_INTERVAL: Duration = Duration::from_millis(500);

        let mut current = io.read().await.current;
        let max_tries = (timeout.as_secs_f64() / SLEEP_INTERVAL.as_secs_f64()).ceil() as u64;
        let mut try_num = 0;
        loop {
            tokio::time::sleep(SLEEP_INTERVAL).await;
            try_num += 1;
            if try_num >= max_tries {
                anyhow::bail!("Timeout");
            }
            let new_current = io.read().await.current;
            // Ensure we did not go back in latest processed batch
            if new_current < current {
                anyhow::bail!(
                    "Latest processed batch regressed to #{} back from #{}",
                    new_current,
                    current
                );
            }
            current = new_current;
            if current >= l1_batch_number {
                return Ok(());
            }
        }
    }
}

#[derive(Debug)]
struct TestOutputFactory {
    delays: HashMap<L1BatchNumber, Duration>,
}

#[async_trait]
impl OutputHandlerFactory for TestOutputFactory {
    async fn create_handler(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>> {
        let delay = self.delays.get(&l1_batch_number).copied();
        #[derive(Debug)]
        struct TestOutputHandler {
            delay: Option<Duration>,
        }
        #[async_trait]
        impl StateKeeperOutputHandler for TestOutputHandler {
            async fn handle_l2_block(
                &mut self,
                _updates_manager: &UpdatesManager,
            ) -> anyhow::Result<()> {
                Ok(())
            }

            async fn handle_l1_batch(
                &mut self,
                _updates_manager: Arc<UpdatesManager>,
            ) -> anyhow::Result<()> {
                if let Some(delay) = self.delay {
                    tokio::time::sleep(delay).await
                }
                Ok(())
            }
        }
        Ok(Box::new(TestOutputHandler { delay }))
    }
}

/// Creates an L2 transaction with randomized parameters.
pub fn create_l2_transaction(
    account: &mut Account,
    fee_per_gas: u64,
    gas_per_pubdata: u64,
) -> L2Tx {
    let fee = Fee {
        gas_limit: (get_intrinsic_constants().l2_tx_intrinsic_gas * 10).into(),
        max_fee_per_gas: fee_per_gas.into(),
        max_priority_fee_per_gas: 0_u64.into(),
        gas_per_pubdata_limit: gas_per_pubdata.into(),
    };
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Address::random(),
            calldata: vec![],
            value: Default::default(),
            factory_deps: vec![],
        },
        Some(fee),
    );
    L2Tx::try_from(tx).unwrap()
}

async fn store_l1_batches(
    conn: &mut Connection<'_, Core>,
    numbers: ops::RangeInclusive<u32>,
    contract_hashes: BaseSystemContractsHashes,
    accounts: &mut [Account],
) -> anyhow::Result<Vec<L1BatchHeader>> {
    let mut rng = rand::thread_rng();
    let mut batches = Vec::new();
    let mut l2_block_number = conn
        .blocks_dal()
        .get_last_sealed_l2_block_header()
        .await?
        .map(|m| m.number)
        .unwrap_or_default()
        + 1;
    let mut last_l2_block_hash = if l2_block_number == 1.into() {
        // First L2 block ever has a special `prev_l2_block_hash`
        L2BlockHasher::legacy_hash(L2BlockNumber(0))
    } else {
        conn.blocks_dal()
            .get_l2_block_header(l2_block_number - 1)
            .await?
            .unwrap()
            .hash
    };
    for l1_batch_number in numbers {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let account = accounts.choose_mut(&mut rng).unwrap();
        let tx = create_l2_transaction(account, 1000000, 100);
        conn.transactions_dal()
            .insert_transaction_l2(&tx, TransactionExecutionMetrics::default())
            .await?;
        let mut logs = Vec::new();
        let mut written_keys = Vec::new();
        for _ in 0..10 {
            let key = StorageKey::new(AccountTreeId::new(H160::random()), H256::random());
            let value = StorageValue::random();
            written_keys.push(key);
            logs.push(StorageLog {
                kind: StorageLogKind::RepeatedWrite,
                key,
                value,
            });
        }
        let mut factory_deps = HashMap::new();
        for _ in 0..10 {
            factory_deps.insert(H256::random(), rng.gen::<[u8; 32]>().into());
        }
        conn.storage_logs_dal()
            .insert_storage_logs(l2_block_number, &logs)
            .await?;
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(l1_batch_number, &written_keys)
            .await?;
        conn.factory_deps_dal()
            .insert_factory_deps(l2_block_number, &factory_deps)
            .await?;
        let mut new_l2_block = create_l2_block(l2_block_number.0);

        let mut digest = L2BlockHasher::new(
            new_l2_block.number,
            new_l2_block.timestamp,
            last_l2_block_hash,
        );
        digest.push_tx_hash(tx.hash());
        new_l2_block.hash = digest.finalize(ProtocolVersionId::latest());

        l2_block_number += 1;
        new_l2_block.base_system_contracts_hashes = contract_hashes;
        new_l2_block.l2_tx_count = 1;
        conn.blocks_dal().insert_l2_block(&new_l2_block).await?;
        last_l2_block_hash = new_l2_block.hash;
        let tx_result = execute_l2_transaction(tx.clone());
        conn.transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                new_l2_block.number,
                &[tx_result.clone()],
                1.into(),
                ProtocolVersionId::latest(),
                false,
            )
            .await?;

        // Insert a fictive L2 block at the end of the batch
        let mut fictive_l2_block = create_l2_block(l2_block_number.0);
        let mut digest = L2BlockHasher::new(
            fictive_l2_block.number,
            fictive_l2_block.timestamp,
            last_l2_block_hash,
        );
        digest.push_tx_hash(tx.hash());
        fictive_l2_block.hash = digest.finalize(ProtocolVersionId::latest());
        l2_block_number += 1;
        conn.blocks_dal().insert_l2_block(&fictive_l2_block).await?;
        last_l2_block_hash = fictive_l2_block.hash;

        let header = L1BatchHeader::new(
            l1_batch_number,
            l2_block_number.0 as u64 - 2, // Matches the first L2 block in the batch
            BaseSystemContractsHashes::default(),
            ProtocolVersionId::default(),
        );
        let predicted_gas = BlockGasCount {
            commit: 2,
            prove: 3,
            execute: 10,
        };
        conn.blocks_dal()
            .insert_l1_batch(&header, &[], predicted_gas, &[], &[], Default::default())
            .await?;
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_number)
            .await?;
        conn.transactions_dal()
            .mark_txs_as_executed_in_l1_batch(l1_batch_number, &[tx_result])
            .await?;

        let metadata = create_l1_batch_metadata(l1_batch_number.0);
        conn.blocks_dal()
            .save_l1_batch_tree_data(l1_batch_number, &metadata.tree_data())
            .await?;
        conn.blocks_dal()
            .save_l1_batch_commitment_artifacts(
                l1_batch_number,
                &l1_batch_metadata_to_commitment_artifacts(&metadata),
            )
            .await?;
        batches.push(header);
    }

    Ok(batches)
}

async fn fund(pool: &ConnectionPool<Core>, accounts: &[Account]) {
    let mut conn = pool.connection().await.unwrap();

    let eth_amount = U256::from(10).pow(U256::from(32)); //10^32 wei

    for account in accounts {
        let key = storage_key_for_standard_token_balance(
            AccountTreeId::new(L2_BASE_TOKEN_ADDRESS),
            &account.address,
        );
        let value = u256_to_h256(eth_amount);
        let storage_log = StorageLog::new_write_log(key, value);

        conn.storage_logs_dal()
            .append_storage_logs(L2BlockNumber(0), &[storage_log])
            .await
            .unwrap();
        if conn
            .storage_logs_dedup_dal()
            .filter_written_slots(&[storage_log.key.hashed_key()])
            .await
            .unwrap()
            .is_empty()
        {
            conn.storage_logs_dedup_dal()
                .insert_initial_writes(L1BatchNumber(0), &[storage_log.key])
                .await
                .unwrap();
        }
    }
}
