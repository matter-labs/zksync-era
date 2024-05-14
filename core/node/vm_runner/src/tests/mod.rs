use std::{collections::HashMap, ops, sync::Arc, time::Duration};

use async_trait::async_trait;
use multivm::zk_evm_latest::ethereum_types::{H160, H256};
use rand::Rng;
use tokio::sync::RwLock;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_core::state_keeper::{StateKeeperOutputHandler, UpdatesManager};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_node_test_utils::{
    create_l1_batch_metadata, create_l2_block, create_l2_transaction, execute_l2_transaction,
    l1_batch_metadata_to_commitment_artifacts,
};
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader, L2BlockHasher},
    fee::TransactionExecutionMetrics,
    AccountTreeId, L1BatchNumber, ProtocolVersionId, StorageKey, StorageLog, StorageLogKind,
    StorageValue,
};

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

async fn store_l2_blocks(
    conn: &mut Connection<'_, Core>,
    numbers: ops::RangeInclusive<u32>,
    contract_hashes: BaseSystemContractsHashes,
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
    let mut last_l2_block = conn
        .blocks_dal()
        .get_l2_block_header(l2_block_number - 1)
        .await?
        .unwrap();
    for l1_batch_number in numbers {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let tx = create_l2_transaction(10, 100);
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
                kind: StorageLogKind::Write,
                key,
                value,
            });
        }
        let mut factory_deps = HashMap::new();
        for _ in 0..10 {
            factory_deps.insert(H256::random(), rng.gen::<[u8; 32]>().into());
        }
        conn.storage_logs_dal()
            .insert_storage_logs(l2_block_number, &[(tx.hash(), logs)])
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
            last_l2_block.hash,
        );
        digest.push_tx_hash(tx.hash());
        new_l2_block.hash = digest.finalize(ProtocolVersionId::latest());

        l2_block_number += 1;
        new_l2_block.base_system_contracts_hashes = contract_hashes;
        new_l2_block.l2_tx_count = 1;
        conn.blocks_dal().insert_l2_block(&new_l2_block).await?;
        last_l2_block = new_l2_block.clone();
        let tx_result = execute_l2_transaction(tx);
        conn.transactions_dal()
            .mark_txs_as_executed_in_l2_block(new_l2_block.number, &[tx_result.clone()], 1.into())
            .await?;

        // Insert a fictive L2 block at the end of the batch
        // let fictive_l2_block = create_l2_block(l2_block_number.0);
        // l2_block_number += 1;
        // conn.blocks_dal().insert_l2_block(&fictive_l2_block).await?;

        let header = L1BatchHeader::new(
            l1_batch_number,
            l2_block_number.0 as u64 - 1, // Matches the first L2 block in the batch
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
