use std::{default::Default, fs, path::PathBuf, sync::Arc};

use tempfile::TempDir;
use tokio::sync::watch;
use zksync_basic_types::{Address, L1BatchNumber, L2BlockNumber, H256, U256};
use zksync_dal::{eth_watcher_dal::EventType, ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_object_store::ObjectStore;
use zksync_types::{
    aggregated_operations::L1BatchAggregatedActionType,
    block::{L1BatchHeader, L1BatchTreeData, L2BlockHasher, L2BlockHeader, UnsealedL1BatchHeader},
    commitment::{L1BatchCommitmentArtifacts, L1BatchCommitmentHash},
    eth_sender::EthTxFinalityStatus,
    fee_model::{BatchFeeInput, L1PeggedBatchFeeModelInput},
    snapshots::SnapshotFactoryDependencies,
    tokens::{TokenInfo, TokenMetadata},
    Execute, ExecuteTransactionCommon, Transaction, ETHEREUM_ADDRESS,
};
use zksync_vm_interface::{
    CircuitStatistic, L2Block, TransactionExecutionResult, TxExecutionStatus, VmExecutionMetrics,
};
use zksync_web3_decl::client::{DynClient, L1};

use crate::{
    l1_fetcher::{
        blob_http_client::BlobClient,
        constants::{initial_states_directory, sepolia_versioning},
        fetcher::{L1Fetcher, L1FetcherConfig, ProtocolVersioning::OnlyV3},
        types::CommitBlock,
    },
    processor::snapshot::StateCompressor,
};

pub async fn recover_latest_l1_batch(
    last_block: CommitBlock,
    connection_pool: ConnectionPool<Core>,
) {
    let mut storage = connection_pool.connection().await.unwrap();
    // This node already has at least one L1 batch in DB, nothing to do here!
    if storage
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .is_some()
    {
        return;
    }
    let latest_protocol_version = storage
        .protocol_versions_dal()
        .latest_semantic_version()
        .await
        .unwrap()
        .unwrap();

    let snapshot_recovery = storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await
        .unwrap()
        .unwrap();

    let mut l2_block_header = L2BlockHeader {
        number: snapshot_recovery.l2_block_number - 1,
        timestamp: snapshot_recovery.l2_block_timestamp - 1,
        hash: L2BlockHasher::legacy_hash(L2BlockNumber(snapshot_recovery.l2_block_number.0) - 1),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Default::default(),
        base_fee_per_gas: 0,
        batch_fee_input: BatchFeeInput::L1Pegged(L1PeggedBatchFeeModelInput {
            fair_l2_gas_price: 0,
            l1_gas_price: 0,
        }),
        gas_per_pubdata_limit: 0,
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(latest_protocol_version.minor),
        virtual_blocks: 0,
        gas_limit: 0,
        logs_bloom: Default::default(),
        pubdata_params: Default::default(),
        rolling_txs_hash: None,
    };
    tracing::info!(
        "Reconstructed previous l2 block {} with hash {:?}",
        l2_block_header.number,
        l2_block_header.hash
    );
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block_header)
        .await
        .unwrap();
    l2_block_header.hash = snapshot_recovery.l2_block_hash;
    l2_block_header.number = snapshot_recovery.l2_block_number;
    l2_block_header.timestamp = snapshot_recovery.l2_block_timestamp;
    tracing::info!(
        "Reconstructed latest l2 block {} with hash {:?}",
        l2_block_header.number,
        l2_block_header.hash
    );
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block_header)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .insert_l1_batch(UnsealedL1BatchHeader {
            number: snapshot_recovery.l1_batch_number,
            timestamp: snapshot_recovery.l1_batch_timestamp,
            protocol_version: Some(latest_protocol_version.minor),
            fee_address: Default::default(),
            fee_input: BatchFeeInput::L1Pegged(L1PeggedBatchFeeModelInput {
                fair_l2_gas_price: 0,
                l1_gas_price: 0,
            }),
            pubdata_limit: None,
        })
        .await
        .unwrap();
    // todo verify pubdata limit
    storage
        .blocks_dal()
        .mark_l1_batch_as_sealed(
            &L1BatchHeader {
                number: snapshot_recovery.l1_batch_number,
                timestamp: snapshot_recovery.l1_batch_timestamp,
                l1_tx_count: last_block.l1_tx_count as u16,
                l2_tx_count: 0,
                priority_ops_onchain_data: last_block.priority_ops_onchain_data,
                l2_to_l1_logs: vec![],
                l2_to_l1_messages: vec![],
                bloom: Default::default(),
                used_contract_hashes: vec![],
                base_system_contracts_hashes: Default::default(),
                system_logs: vec![],
                protocol_version: Some(latest_protocol_version.minor),
                pubdata_input: None,
                fee_address: Default::default(),
                batch_fee_input: Default::default(),
                pubdata_limit: None,
            },
            &[],
            &[],
            &[],
            CircuitStatistic::default(),
            0,
        )
        .await
        .unwrap();
    tracing::info!("leaf_index: {}", last_block.rollup_last_leaf_index);
    storage
        .blocks_dal()
        .save_l1_batch_tree_data(
            snapshot_recovery.l1_batch_number,
            &L1BatchTreeData {
                hash: snapshot_recovery.l1_batch_root_hash,
                rollup_last_leaf_index: last_block.rollup_last_leaf_index,
            },
        )
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(snapshot_recovery.l1_batch_number)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(
            snapshot_recovery.l1_batch_number,
            &L1BatchCommitmentArtifacts {
                commitment_hash: L1BatchCommitmentHash {
                    pass_through_data: Default::default(),
                    aux_output: Default::default(),
                    meta_parameters: Default::default(),
                    commitment: last_block.commitment,
                },
                l2_l1_merkle_root: last_block.l2_logs_tree_root,
                compressed_state_diffs: None,
                compressed_initial_writes: None,
                compressed_repeated_writes: None,
                zkporter_is_available: false,
                aux_commitments: None,
                aggregation_root: Default::default(),
                local_root: Default::default(),
                state_diff_hash: Default::default(),
            },
        )
        .await
        .unwrap();
    storage
        .vm_runner_dal()
        .mark_protective_reads_batch_as_processing(snapshot_recovery.l1_batch_number)
        .await
        .unwrap();
    storage
        .vm_runner_dal()
        .mark_protective_reads_batch_as_completed(snapshot_recovery.l1_batch_number)
        .await
        .unwrap();

    let eth_token = TokenInfo {
        l1_address: ETHEREUM_ADDRESS,
        l2_address: ETHEREUM_ADDRESS,
        metadata: TokenMetadata {
            name: "Ether".to_string(),
            symbol: "ETH".to_string(),
            decimals: 18,
        },
    };

    storage.tokens_dal().add_tokens(&[eth_token]).await.unwrap();
}

pub async fn recover_eth_sender(
    connection_pool: ConnectionPool<Core>,
    l1_client: Box<DynClient<L1>>,
    diamond_proxy_addr: Address,
) {
    let l1_fetcher = L1Fetcher::new(
        L1FetcherConfig {
            block_step: 10000,
            diamond_proxy_addr,
            versioning: OnlyV3,
        },
        l1_client.clone(),
    )
    .unwrap();
    let last_l1_batch_number = l1_fetcher
        .get_last_executed_l1_batch_number()
        .await
        .unwrap();
    let mut storage = connection_pool.connection().await.unwrap();
    let chain_id = Some(l1_client.fetch_chain_id().await.unwrap());
    let commit_tx_hash = H256::random();
    storage
        .eth_sender_dal()
        .insert_pending_received_eth_tx(
            last_l1_batch_number,
            L1BatchAggregatedActionType::Commit,
            commit_tx_hash,
            chain_id,
        )
        .await
        .unwrap();
    storage
        .eth_sender_dal()
        .confirm_tx(commit_tx_hash, EthTxFinalityStatus::Finalized, U256::zero())
        .await
        .unwrap();

    let prover_tx_hash = H256::random();
    storage
        .eth_sender_dal()
        .insert_pending_received_eth_tx(
            last_l1_batch_number,
            L1BatchAggregatedActionType::PublishProofOnchain,
            prover_tx_hash,
            chain_id,
        )
        .await
        .unwrap();
    storage
        .eth_sender_dal()
        .confirm_tx(prover_tx_hash, EthTxFinalityStatus::Finalized, U256::zero())
        .await
        .unwrap();

    let execute_tx_hash = H256::random();
    storage
        .eth_sender_dal()
        .insert_pending_received_eth_tx(
            last_l1_batch_number,
            L1BatchAggregatedActionType::Execute,
            execute_tx_hash,
            chain_id,
        )
        .await
        .unwrap();
    storage
        .eth_sender_dal()
        .confirm_tx(
            execute_tx_hash,
            EthTxFinalityStatus::Finalized,
            U256::zero(),
        )
        .await
        .unwrap();
}

pub async fn recover_latest_priority_tx(
    connection_pool: ConnectionPool<Core>,
    l1_client: Box<DynClient<L1>>,
    diamond_proxy_addr: Address,
) {
    let l1_fetcher = L1Fetcher::new(
        L1FetcherConfig {
            block_step: 10000,
            diamond_proxy_addr,
            versioning: OnlyV3,
        },
        l1_client.clone(),
    )
    .unwrap();
    let mut storage = connection_pool.connection().await.unwrap();
    let latest_protocol_version = storage
        .protocol_versions_dal()
        .latest_semantic_version()
        .await
        .unwrap()
        .unwrap();

    if storage
        .transactions_dal()
        .last_priority_id()
        .await
        .unwrap()
        .is_none()
    {
        let last_l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap()
            .unwrap();
        let last_processed_priority_tx = l1_fetcher
            .get_last_processed_priority_transaction(last_l1_batch_number)
            .await;
        let block = last_processed_priority_tx.eth_block() + 1;
        storage
            .transactions_dal()
            .insert_transaction_l1(&last_processed_priority_tx, block)
            .await
            .unwrap();
        let tx_result = TransactionExecutionResult {
            transaction: Transaction {
                common_data: ExecuteTransactionCommon::L1(
                    last_processed_priority_tx.common_data.clone(),
                ),
                execute: Execute::default(),
                received_timestamp_ms: 0,
                raw_bytes: None,
            },
            hash: last_processed_priority_tx.hash(),
            execution_info: VmExecutionMetrics::default(),
            execution_status: TxExecutionStatus::Success,
            refunded_gas: 0,
            call_traces: vec![],
            revert_reason: None,
        };
        let last_miniblock = storage
            .blocks_dal()
            .get_last_sealed_l2_block_header()
            .await
            .unwrap()
            .unwrap()
            .number;
        storage
            .transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                last_miniblock,
                &[tx_result.clone()],
                U256::zero(),
                latest_protocol_version.minor,
                false,
            )
            .await
            .unwrap();
        storage
            .transactions_dal()
            .mark_txs_as_executed_in_l1_batch(last_l1_batch_number, &[tx_result.hash])
            .await
            .unwrap();
        tracing::info!(
            "Inserted bogus priority transaction with id {:?}",
            last_processed_priority_tx.common_data.serial_id
        );
    }
}

pub async fn recover_eth_watch(
    connection_pool: ConnectionPool<Core>,
    l1_client: Box<DynClient<L1>>,
) {
    let mut storage = connection_pool.connection().await.unwrap();

    // If there are no priority transactions in the database, we need to find last priority transaction number on L1 and insert it

    let chain_id = l1_client.fetch_chain_id().await.unwrap();
    let block = storage
        .transactions_dal()
        .get_last_processed_l1_block()
        .await
        .unwrap()
        .unwrap();

    storage
        .eth_watcher_dal()
        .get_or_set_next_block_to_process(EventType::PriorityTransactions, chain_id, block.0 as u64)
        .await
        .unwrap();
    storage
        .eth_watcher_dal()
        .get_or_set_next_block_to_process(EventType::ProtocolUpgrades, chain_id, block.0 as u64)
        .await
        .unwrap();
    tracing::info!("Recovered eth_watch state, last processed block is {block}")
}

pub async fn recover_latest_protocol_version(
    connection_pool: ConnectionPool<Core>,
    l1_client: Box<DynClient<L1>>,
    diamond_proxy_addr: Address,
    l1_batch_number: L1BatchNumber,
) {
    let l1_fetcher = L1Fetcher::new(
        L1FetcherConfig {
            block_step: 10000,
            diamond_proxy_addr,
            versioning: OnlyV3,
        },
        l1_client.clone(),
    )
    .unwrap();
    let latest_version = l1_fetcher
        .get_latest_protocol_version(l1_batch_number)
        .await;
    tracing::info!("Recovered protocol version is {:?}", latest_version);
    let mut storage = connection_pool.connection().await.unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version(
            latest_version.version,
            latest_version.timestamp,
            latest_version.l1_verifier_config,
            latest_version.base_system_contracts_hashes,
            None,
        )
        .await
        .unwrap();
}

pub async fn find_matching_genesis_state_file(l1_fetcher: &L1Fetcher) -> PathBuf {
    let genesis_hash = l1_fetcher.get_genesis_root_hash().await;
    for entry in fs::read_dir(initial_states_directory()).unwrap() {
        let entry = entry.unwrap();
        let temp_dir = TempDir::new().unwrap().into_path().join("db");
        let mut processor = StateCompressor::new(temp_dir).await;
        processor.process_genesis_state(entry.path()).await;
        if processor.get_root_hash() == genesis_hash {
            return entry.path();
        }
    }
    panic!("No matching genesis state file found for hash {genesis_hash:?}!")
}
pub async fn create_l1_snapshot(
    l1_client: Box<DynClient<L1>>,
    blob_client: &Arc<dyn BlobClient>,
    object_store: &Arc<dyn ObjectStore>,
    diamond_proxy_addr: Address,
    stop_receiver: &watch::Receiver<bool>,
) -> (CommitBlock, L2Block, u64) {
    let temp_dir = TempDir::new().unwrap().into_path().join("db");

    let l1_fetcher = L1Fetcher::new(
        L1FetcherConfig {
            block_step: 50_000,
            diamond_proxy_addr,
            versioning: sepolia_versioning(),
        },
        l1_client,
    )
    .unwrap();
    let initial_state_path = find_matching_genesis_state_file(&l1_fetcher).await;
    tracing::info!("Using genesis state file {:?}", initial_state_path);
    let blocks = l1_fetcher
        .get_all_blocks_to_process(blob_client, Some(object_store), stop_receiver)
        .await;
    let last_block = blocks.last().unwrap().clone();
    let last_l1_batch_number = L1BatchNumber(last_block.l1_batch_number as u32);
    let mut processor = StateCompressor::new(temp_dir).await;
    processor.disabled_tree();
    processor.process_genesis_state(initial_state_path).await;
    processor.process_blocks(blocks, stop_receiver).await;

    tracing::info!("Processing L1 data finished");

    let chunks_count = processor
        .dump_storage_logs_chunked(last_l1_batch_number, object_store)
        .await;

    let factory_deps = SnapshotFactoryDependencies {
        factory_deps: processor.export_factory_deps().await,
    };
    object_store
        .put(last_l1_batch_number, &factory_deps)
        .await
        .unwrap();
    tracing::info!("Dumped {} factory deps", factory_deps.factory_deps.len());

    (
        last_block,
        processor.read_latest_miniblock_metadata(),
        chunks_count,
    )
}
#[cfg(test)]
mod tests {
    use crate::{
        l1_fetcher::constants::{sepolia_initial_state_path, sepolia_l1_fetcher},
        processor::db_recovery::find_matching_genesis_state_file,
    };

    #[test_log::test(tokio::test)]
    async fn finding_matching_genesis_state_file_works_for_sepolia() {
        let l1_fetcher = sepolia_l1_fetcher();
        let path = find_matching_genesis_state_file(&l1_fetcher).await;
        assert_eq!(path, sepolia_initial_state_path());
    }
}
