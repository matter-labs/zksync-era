use anyhow::Context;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_multivm::interface::VmEvent;
use zksync_system_constants::{CONTRACT_DEPLOYER_ADDRESS, L2_NATIVE_TOKEN_VAULT_ADDRESS};
use zksync_types::{
    ethabi, h256_to_address,
    tokens::{TokenInfo, TokenMetadata},
    Address, L2BlockNumber, H256,
};

use crate::{
    io::seal_logic::SealStrategy,
    metrics::{L2BlockSealStage, L2_BLOCK_METRICS},
    updates::L2BlockSealCommand,
};

fn extract_added_tokens(
    l2_token_deployer_addr: Address,
    all_generated_events: &[VmEvent],
) -> Vec<TokenInfo> {
    let deployed_tokens = all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the deployer contract that match the expected signature.
            event.address == CONTRACT_DEPLOYER_ADDRESS
                && event.indexed_topics.len() == 4
                && event.indexed_topics[0] == VmEvent::DEPLOY_EVENT_SIGNATURE
                && h256_to_address(&event.indexed_topics[1]) == l2_token_deployer_addr
        })
        .map(|event| h256_to_address(&event.indexed_topics[3]));

    extract_added_token_info_from_addresses(all_generated_events, deployed_tokens)
}

fn extract_added_token_info_from_addresses(
    all_generated_events: &[VmEvent],
    deployed_tokens: impl Iterator<Item = Address>,
) -> Vec<TokenInfo> {
    static BRIDGE_INITIALIZATION_SIGNATURE_OLD: Lazy<H256> = Lazy::new(|| {
        ethabi::long_signature(
            "BridgeInitialization",
            &[
                ethabi::ParamType::Address,
                ethabi::ParamType::String,
                ethabi::ParamType::String,
                ethabi::ParamType::Uint(8),
            ],
        )
    });

    static BRIDGE_INITIALIZATION_SIGNATURE_NEW: Lazy<H256> = Lazy::new(|| {
        ethabi::long_signature(
            "BridgeInitialize",
            &[
                ethabi::ParamType::Address,
                ethabi::ParamType::String,
                ethabi::ParamType::String,
                ethabi::ParamType::Uint(8),
            ],
        )
    });

    deployed_tokens
        .filter_map(|l2_token_address| {
            all_generated_events
                .iter()
                .find(|event| {
                    event.address == l2_token_address
                        && (event.indexed_topics[0] == *BRIDGE_INITIALIZATION_SIGNATURE_NEW
                            || event.indexed_topics[0] == *BRIDGE_INITIALIZATION_SIGNATURE_OLD)
                })
                .map(|event| {
                    let l1_token_address = h256_to_address(&event.indexed_topics[1]);
                    let mut dec_ev = ethabi::decode(
                        &[
                            ethabi::ParamType::String,
                            ethabi::ParamType::String,
                            ethabi::ParamType::Uint(8),
                        ],
                        &event.value,
                    )
                    .unwrap();

                    TokenInfo {
                        l1_address: l1_token_address,
                        l2_address: l2_token_address,
                        metadata: TokenMetadata {
                            name: dec_ev.remove(0).into_string().unwrap(),
                            symbol: dec_ev.remove(0).into_string().unwrap(),
                            decimals: dec_ev.remove(0).into_uint().unwrap().as_u32() as u8,
                        },
                    }
                })
        })
        .collect()
}

/// Helper struct that encapsulates parallel l2 block sealing logic.
#[derive(Debug)]
pub struct L2BlockSealProcess;

impl L2BlockSealProcess {
    pub fn all_subtasks() -> Vec<Box<dyn L2BlockSealSubtask>> {
        vec![
            Box::new(MarkTransactionsInL2BlockSubtask),
            Box::new(InsertStorageLogsSubtask),
            Box::new(InsertFactoryDepsSubtask),
            Box::new(InsertTokensSubtask),
            Box::new(InsertEventsSubtask),
            Box::new(InsertL2ToL1LogsSubtask),
        ]
    }

    pub fn subtasks_len() -> u32 {
        Self::all_subtasks().len() as u32
    }

    pub async fn run_subtasks(
        command: &L2BlockSealCommand,
        strategy: &mut SealStrategy<'_>,
    ) -> anyhow::Result<()> {
        let subtasks = Self::all_subtasks();
        match strategy {
            SealStrategy::Sequential(connection) => {
                for subtask in subtasks {
                    let subtask_name = subtask.name();
                    subtask
                        .run(command, connection)
                        .await
                        .context(subtask_name)?;
                }
            }
            SealStrategy::Parallel(pool) => {
                let pool = &*pool;
                let handles = subtasks.into_iter().map(|subtask| {
                    let subtask_name = subtask.name();
                    async move {
                        let mut connection = pool.connection_tagged("state_keeper").await?;
                        subtask
                            .run(command, &mut connection)
                            .await
                            .context(subtask_name)
                    }
                });
                futures::future::try_join_all(handles).await?;
            }
        }

        Ok(())
    }

    /// Clears pending l2 block data from the database.
    pub async fn clear_pending_l2_block(
        connection: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        let seal_subtasks = L2BlockSealProcess::all_subtasks();
        for subtask in seal_subtasks {
            subtask.rollback(connection, last_sealed_l2_block).await?;
        }

        Ok(())
    }
}

/// An abstraction that represents l2 block seal sub-task that can be run in parallel with other sub-tasks.
#[async_trait::async_trait]
pub trait L2BlockSealSubtask: Send + Sync + 'static {
    /// Returns sub-task name.
    fn name(&self) -> &'static str;

    /// Runs seal process.
    async fn run(
        self: Box<Self>,
        command: &L2BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()>;

    /// Rollbacks data that was saved to database for the pending L2 block.
    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub(super) struct MarkTransactionsInL2BlockSubtask;

#[async_trait]
impl L2BlockSealSubtask for MarkTransactionsInL2BlockSubtask {
    fn name(&self) -> &'static str {
        "mark_transactions_in_l2_block"
    }

    async fn run(
        self: Box<Self>,
        command: &L2BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let progress = L2_BLOCK_METRICS.start(
            L2BlockSealStage::MarkTransactionsInL2Block,
            command.is_l2_block_fictive(),
        );

        connection
            .transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                command.l2_block.number,
                &command.l2_block.executed_transactions,
                command.base_fee_per_gas.into(),
                command.l2_block.protocol_version,
                command.pre_insert_txs,
            )
            .await?;

        progress.observe(command.l2_block.executed_transactions.len());
        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .transactions_dal()
            .reset_transactions_state(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertStorageLogsSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertStorageLogsSubtask {
    fn name(&self) -> &'static str {
        "insert_storage_logs"
    }

    async fn run(
        self: Box<Self>,
        command: &L2BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let is_fictive = command.is_l2_block_fictive();
        let write_logs = command.extract_deduplicated_write_logs();

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertStorageLogs, is_fictive);

        connection
            .storage_logs_dal()
            .insert_storage_logs(command.l2_block.number, &write_logs)
            .await?;

        progress.observe(write_logs.len());
        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .storage_logs_dal()
            .roll_back_storage_logs(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertFactoryDepsSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertFactoryDepsSubtask {
    fn name(&self) -> &'static str {
        "insert_factory_deps"
    }

    async fn run(
        self: Box<Self>,
        command: &L2BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let progress = L2_BLOCK_METRICS.start(
            L2BlockSealStage::InsertFactoryDeps,
            command.is_l2_block_fictive(),
        );

        if !command.l2_block.new_factory_deps.is_empty() {
            connection
                .factory_deps_dal()
                .insert_factory_deps(command.l2_block.number, &command.l2_block.new_factory_deps)
                .await?;
        }
        progress.observe(command.l2_block.new_factory_deps.len());

        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .factory_deps_dal()
            .roll_back_factory_deps(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertTokensSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertTokensSubtask {
    fn name(&self) -> &'static str {
        "insert_tokens"
    }

    async fn run(
        self: Box<Self>,
        command: &L2BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let is_fictive = command.is_l2_block_fictive();
        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::ExtractAddedTokens, is_fictive);
        let token_deployer_address = command
            .l2_legacy_shared_bridge_addr
            .unwrap_or(L2_NATIVE_TOKEN_VAULT_ADDRESS);
        let added_tokens = extract_added_tokens(token_deployer_address, &command.l2_block.events);
        progress.observe(added_tokens.len());

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertTokens, is_fictive);
        if !added_tokens.is_empty() {
            connection.tokens_dal().add_tokens(&added_tokens).await?;
        }
        progress.observe(added_tokens.len());

        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .tokens_dal()
            .roll_back_tokens(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertEventsSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertEventsSubtask {
    fn name(&self) -> &'static str {
        "insert_events"
    }

    async fn run(
        self: Box<Self>,
        command: &L2BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let is_fictive = command.is_l2_block_fictive();
        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::ExtractEvents, is_fictive);
        let l2_block_events = command.extract_events(is_fictive);
        let l2_block_event_count: usize =
            l2_block_events.iter().map(|(_, events)| events.len()).sum();
        progress.observe(l2_block_event_count);

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertEvents, is_fictive);
        connection
            .events_dal()
            .save_events(command.l2_block.number, &l2_block_events)
            .await?;
        progress.observe(l2_block_event_count);
        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .events_dal()
            .roll_back_events(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertL2ToL1LogsSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertL2ToL1LogsSubtask {
    fn name(&self) -> &'static str {
        "insert_l2_to_l1_logs"
    }

    async fn run(
        self: Box<Self>,
        command: &L2BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let is_fictive = command.is_l2_block_fictive();
        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::ExtractL2ToL1Logs, is_fictive);

        let user_l2_to_l1_logs = command.extract_user_l2_to_l1_logs(is_fictive);
        let user_l2_to_l1_log_count: usize = user_l2_to_l1_logs
            .iter()
            .map(|(_, l2_to_l1_logs)| l2_to_l1_logs.len())
            .sum();

        progress.observe(user_l2_to_l1_log_count);

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertL2ToL1Logs, is_fictive);
        if !user_l2_to_l1_logs.is_empty() {
            connection
                .events_dal()
                .save_user_l2_to_l1_logs(command.l2_block.number, &user_l2_to_l1_logs)
                .await?;
        }
        progress.observe(user_l2_to_l1_log_count);
        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .events_dal()
            .roll_back_l2_to_l1_logs(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use zksync_dal::{ConnectionPool, Core};
    use zksync_multivm::{
        interface::{tracer::ValidationTraces, TransactionExecutionResult, TxExecutionStatus},
        utils::{get_max_batch_gas_limit, get_max_gas_per_pubdata_byte},
        zk_evm_latest::ethereum_types::H256,
        VmVersion,
    };
    use zksync_node_test_utils::create_l2_transaction;
    use zksync_types::{
        block::L2BlockHeader,
        commitment::PubdataParams,
        h256_to_u256,
        l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
        AccountTreeId, Address, L1BatchNumber, ProtocolVersionId, StorageKey, StorageLog,
        StorageLogKind, StorageLogWithPreviousValue,
    };

    use super::*;
    use crate::updates::L2BlockUpdates;

    #[tokio::test]
    async fn rollback_pending_l2_block() {
        let pool =
            ConnectionPool::<Core>::constrained_test_pool(L2BlockSealProcess::subtasks_len()).await;

        // Prepare data.
        let tx = create_l2_transaction(1, 1);
        pool.connection()
            .await
            .unwrap()
            .transactions_dal()
            .insert_transaction_l2(&tx, Default::default(), ValidationTraces::default())
            .await
            .unwrap();
        let tx_hash = tx.hash();
        let executed_transactions = vec![TransactionExecutionResult {
            transaction: tx.into(),
            hash: tx_hash,
            execution_info: Default::default(),
            execution_status: TxExecutionStatus::Success,
            refunded_gas: 0,
            call_traces: Vec::new(),
            revert_reason: None,
        }];
        let events = vec![VmEvent {
            location: (L1BatchNumber(1), 0),
            address: Address::zero(),
            indexed_topics: Vec::new(),
            value: Vec::new(),
        }];
        let storage_key = StorageKey::new(AccountTreeId::new(Address::zero()), H256::zero());
        let storage_value = H256::from_low_u64_be(1);
        let storage_logs = vec![StorageLogWithPreviousValue {
            log: StorageLog {
                key: storage_key,
                value: storage_value,
                kind: StorageLogKind::InitialWrite,
            },
            previous_value: H256::zero(),
        }];
        let user_l2_to_l1_logs = vec![UserL2ToL1Log(L2ToL1Log {
            shard_id: 0,
            is_service: false,
            tx_number_in_block: 0,
            sender: Address::zero(),
            key: H256::zero(),
            value: H256::zero(),
        })];

        let bytecode_hash = H256::repeat_byte(0x12);
        let bytecode = vec![0u8; 32];
        let new_factory_deps = vec![(bytecode_hash, bytecode)].into_iter().collect();
        let l2_block_seal_command = L2BlockSealCommand {
            l1_batch_number: L1BatchNumber(1),
            l2_block: L2BlockUpdates::new_with_data(
                L2BlockNumber(1),
                1000,
                executed_transactions,
                events,
                storage_logs,
                user_l2_to_l1_logs,
                new_factory_deps,
            ),
            first_tx_index: 0,
            fee_account_address: Default::default(),
            fee_input: Default::default(),
            base_fee_per_gas: Default::default(),
            base_system_contracts_hashes: Default::default(),
            protocol_version: Some(ProtocolVersionId::latest()),
            l2_legacy_shared_bridge_addr: Default::default(),
            pre_insert_txs: false,
            pubdata_params: PubdataParams::default(),
            insert_header: false, // Doesn't matter for this test.
        };

        // Run.
        let mut strategy = SealStrategy::Parallel(&pool);
        L2BlockSealProcess::run_subtasks(&l2_block_seal_command, &mut strategy)
            .await
            .unwrap();

        // Check factory dependency is saved.
        let mut connection = pool.connection().await.unwrap();
        let factory_deps = connection
            .factory_deps_dal()
            .get_factory_deps(&vec![bytecode_hash].into_iter().collect())
            .await;
        assert!(factory_deps.contains_key(&h256_to_u256(bytecode_hash)));

        // Rollback.
        L2BlockSealProcess::clear_pending_l2_block(&mut connection, L2BlockNumber(0))
            .await
            .unwrap();

        // Check factory dependency was removed.
        let factory_deps = connection
            .factory_deps_dal()
            .get_factory_deps(&vec![bytecode_hash].into_iter().collect())
            .await;
        assert!(factory_deps.is_empty());
        drop(connection);

        // Run again.
        let mut strategy = SealStrategy::Parallel(&pool);
        L2BlockSealProcess::run_subtasks(&l2_block_seal_command, &mut strategy)
            .await
            .unwrap();

        // Check DAL doesn't return tx receipt before block header is saved.
        let mut connection = pool.connection().await.unwrap();
        let tx_receipts = connection
            .transactions_web3_dal()
            .get_transaction_receipts(&[tx_hash])
            .await
            .unwrap();
        assert!(tx_receipts.is_empty(), "{tx_receipts:?}");

        // Insert block header.
        let l2_block_header = L2BlockHeader {
            number: l2_block_seal_command.l2_block.number,
            timestamp: l2_block_seal_command.l2_block.timestamp(),
            hash: l2_block_seal_command.l2_block.get_l2_block_hash(),
            l1_tx_count: 0,
            l2_tx_count: 1,
            fee_account_address: l2_block_seal_command.fee_account_address,
            base_fee_per_gas: l2_block_seal_command.base_fee_per_gas,
            batch_fee_input: l2_block_seal_command.fee_input,
            base_system_contracts_hashes: l2_block_seal_command.base_system_contracts_hashes,
            protocol_version: l2_block_seal_command.protocol_version,
            gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(VmVersion::latest()),
            virtual_blocks: l2_block_seal_command.l2_block.virtual_blocks,
            gas_limit: get_max_batch_gas_limit(VmVersion::latest()),
            logs_bloom: Default::default(),
            pubdata_params: l2_block_seal_command.pubdata_params,
        };
        connection
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&Default::default())
            .await
            .unwrap();
        connection
            .blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await
            .unwrap();

        // Check tx receipt.
        let tx_receipt = connection
            .transactions_web3_dal()
            .get_transaction_receipts(&[tx_hash])
            .await
            .unwrap()
            .remove(0)
            .inner;
        assert_eq!(tx_receipt.block_number.as_u32(), 1);
        assert_eq!(tx_receipt.logs.len(), 1);
        assert_eq!(tx_receipt.l2_to_l1_logs.len(), 1);
    }
}
