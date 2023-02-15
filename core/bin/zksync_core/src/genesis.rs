//! This module aims to provide a genesis setup for the zkSync Era network.
//! It initializes the Merkle tree with the basic setup (such as fields of special service accounts),
//! setups the required databases, and outputs the data required to initialize a smart contract.

use tempfile::TempDir;
use vm::zk_evm::aux_structures::{LogQuery, Timestamp};
use zksync_types::system_contracts::get_system_smart_contracts;
use zksync_types::tokens::{TokenInfo, TokenMetadata, ETHEREUM_ADDRESS};
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader, MiniblockHeader},
    get_code_key, Address, L1BatchNumber, MiniblockNumber, StorageLog, StorageLogKind,
    StorageLogQueryType, H256,
};
use zksync_types::{get_system_context_init_logs, StorageLogQuery, FAIR_L2_GAS_PRICE};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256, miniblock_hash};

use zksync_config::ZkSyncConfig;
use zksync_merkle_tree::ZkSyncTree;

use zksync_dal::StorageProcessor;
use zksync_storage::db::Database;

use zksync_storage::RocksDB;
use zksync_types::block::DeployedContract;
use zksync_types::commitment::{BlockCommitment, BlockMetadata};
use zksync_types::log_query_sorter::sort_storage_access_queries;

pub async fn ensure_genesis_state(storage: &mut StorageProcessor<'_>, config: ZkSyncConfig) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let mut tree = ZkSyncTree::new(db);

    let mut transaction = storage.start_transaction().await;

    // return if genesis block was already processed
    if !transaction.blocks_dal().is_genesis_needed() {
        vlog::debug!("genesis is not needed!");
        return;
    }
    vlog::info!("running regenesis");

    // For now we consider the operator to be the first validator.
    let first_validator_address = config.eth_sender.sender.operator_commit_eth_addr;
    let chain_id = H256::from_low_u64_be(config.chain.eth.zksync_network_id as u64);

    chain_schema_genesis(&mut transaction, first_validator_address, chain_id).await;
    vlog::info!("chain_schema_genesis is complete");

    let storage_logs =
        crate::metadata_calculator::get_logs_for_l1_batch(&mut transaction, L1BatchNumber(0));
    let metadata = tree.process_block(storage_logs.unwrap().storage_logs);
    let genesis_root_hash = H256::from_slice(&metadata.root_hash);
    let rollup_last_leaf_index = metadata.rollup_last_leaf_index;

    let block_commitment = BlockCommitment::new(
        vec![],
        rollup_last_leaf_index,
        genesis_root_hash,
        vec![],
        vec![],
    );

    operations_schema_genesis(
        &mut transaction,
        &block_commitment,
        genesis_root_hash,
        rollup_last_leaf_index,
    );
    vlog::info!("operations_schema_genesis is complete");

    transaction.commit().await;

    // We need to `println` this value because it will be used to initialize the smart contract.
    println!(
        "CONTRACTS_GENESIS_ROOT=0x{}",
        hex::encode(genesis_root_hash)
    );
    println!(
        "CONTRACTS_GENESIS_BLOCK_COMMITMENT=0x{}",
        hex::encode(block_commitment.hash().commitment)
    );
    println!(
        "CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX={}",
        rollup_last_leaf_index
    );
}

async fn insert_system_contracts(
    storage: &mut StorageProcessor<'_>,
    contracts: Vec<DeployedContract>,
    chain_id: H256,
) {
    let system_context_init_logs = (H256::default(), get_system_context_init_logs(chain_id));

    let storage_logs: Vec<(H256, Vec<StorageLog>)> = contracts
        .clone()
        .into_iter()
        .map(|contract| {
            let hash = hash_bytecode(&contract.bytecode);
            let code_key = get_code_key(contract.account_id.address());

            (
                Default::default(),
                vec![StorageLog::new_write_log(code_key, hash)],
            )
        })
        .chain(Some(system_context_init_logs))
        .collect();

    let mut transaction = storage.start_transaction().await;

    transaction
        .storage_logs_dal()
        .insert_storage_logs(MiniblockNumber(0), &storage_logs);

    // we don't produce proof for the genesis block,
    // but we still need to populate the table
    // to have the correct initial state of the merkle tree
    let log_queries: Vec<StorageLogQuery> = storage_logs
        .iter()
        .enumerate()
        .flat_map(|(tx_index, (_, storage_logs))| {
            storage_logs
                .iter()
                .enumerate()
                .map(move |(log_index, storage_log)| {
                    let log_type = match storage_log.kind {
                        StorageLogKind::Read => StorageLogQueryType::Read,
                        StorageLogKind::Write => StorageLogQueryType::InitialWrite,
                    };
                    StorageLogQuery {
                        log_query: LogQuery {
                            // Monotonically increasing Timestamp. Normally it's generated by the VM, but we don't have a VM in the genesis block.
                            timestamp: Timestamp(((tx_index << 16) + log_index) as u32),
                            tx_number_in_block: tx_index as u16,
                            aux_byte: 0,
                            shard_id: 0,
                            address: *storage_log.key.address(),
                            key: h256_to_u256(*storage_log.key.key()),
                            read_value: h256_to_u256(H256::zero()),
                            written_value: h256_to_u256(storage_log.value),
                            rw_flag: storage_log.kind == StorageLogKind::Write,
                            rollback: false,
                            is_service: false,
                        },
                        log_type,
                    }
                })
                .collect::<Vec<StorageLogQuery>>()
        })
        .collect();

    let (_, deduped_log_queries) = sort_storage_access_queries(&log_queries);

    transaction
        .storage_logs_dedup_dal()
        .insert_storage_logs(L1BatchNumber(0), &deduped_log_queries);

    let (protective_reads, deduplicated_writes): (Vec<_>, Vec<_>) = deduped_log_queries
        .into_iter()
        .partition(|log_query| log_query.log_type == StorageLogQueryType::Read);
    transaction
        .storage_logs_dedup_dal()
        .insert_protective_reads(L1BatchNumber(0), &protective_reads);
    transaction
        .storage_logs_dedup_dal()
        .insert_initial_writes(L1BatchNumber(0), &deduplicated_writes);

    transaction.storage_dal().apply_storage_logs(&storage_logs);

    let factory_deps = contracts
        .into_iter()
        .map(|c| (hash_bytecode(&c.bytecode), c.bytecode))
        .collect();
    transaction
        .storage_dal()
        .insert_factory_deps(MiniblockNumber(0), factory_deps);

    transaction.commit().await;
}

pub(crate) async fn chain_schema_genesis<'a>(
    storage: &mut StorageProcessor<'_>,
    first_validator_address: Address,
    chain_id: H256,
) {
    let mut zero_block_header = L1BatchHeader::new(L1BatchNumber(0), 0, first_validator_address);
    zero_block_header.is_finished = true;

    let zero_miniblock_header = MiniblockHeader {
        number: MiniblockNumber(0),
        timestamp: 0,
        hash: miniblock_hash(MiniblockNumber(0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 0,
        l1_gas_price: 0,
        l2_fair_gas_price: FAIR_L2_GAS_PRICE,
    };

    let mut transaction = storage.start_transaction().await;

    transaction
        .blocks_dal()
        .insert_l1_batch(zero_block_header, BlockGasCount::default());
    transaction
        .blocks_dal()
        .insert_miniblock(zero_miniblock_header);
    transaction
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(0));

    let contracts = get_system_smart_contracts();
    insert_system_contracts(&mut transaction, contracts, chain_id).await;

    add_eth_token(&mut transaction).await;

    transaction.commit().await;
}

pub(crate) async fn add_eth_token(storage: &mut StorageProcessor<'_>) {
    let eth_token = TokenInfo {
        l1_address: ETHEREUM_ADDRESS,
        l2_address: ETHEREUM_ADDRESS,
        metadata: TokenMetadata {
            name: "Ether".to_string(),
            symbol: "ETH".to_string(),
            decimals: 18,
        },
    };

    let mut transaction = storage.start_transaction().await;

    transaction.tokens_dal().add_tokens(vec![eth_token.clone()]);
    transaction
        .tokens_dal()
        .update_well_known_l1_token(&ETHEREUM_ADDRESS, eth_token.metadata);

    transaction.commit().await;
}

pub(crate) fn operations_schema_genesis(
    storage: &mut StorageProcessor<'_>,
    block_commitment: &BlockCommitment,
    genesis_root_hash: H256,
    rollup_last_leaf_index: u64,
) {
    let block_commitment_hash = block_commitment.hash();

    let metadata = BlockMetadata {
        root_hash: genesis_root_hash,
        rollup_last_leaf_index,
        merkle_root_hash: genesis_root_hash,
        initial_writes_compressed: vec![],
        repeated_writes_compressed: vec![],
        commitment: block_commitment_hash.commitment,
        l2_l1_messages_compressed: vec![],
        l2_l1_merkle_root: Default::default(),
        block_meta_params: block_commitment.meta_parameters(),
        aux_data_hash: block_commitment_hash.aux_output,
        meta_parameters_hash: block_commitment_hash.meta_parameters,
        pass_through_data_hash: block_commitment_hash.pass_through_data,
    };
    storage
        .blocks_dal()
        .save_block_metadata(L1BatchNumber(0), metadata);
}
