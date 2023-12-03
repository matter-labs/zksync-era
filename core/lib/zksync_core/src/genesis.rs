//! This module aims to provide a genesis setup for the zkSync Era network.
//! It initializes the Merkle tree with the basic setup (such as fields of special service accounts),
//! setups the required databases, and outputs the data required to initialize a smart contract.

use anyhow::Context as _;

use zksync_contracts::BaseSystemContracts;
use zksync_dal::StorageProcessor;
use zksync_merkle_tree::domain::ZkSyncTree;

use zksync_types::{
    block::DeployedContract,
    block::{legacy_miniblock_hash, BlockGasCount, L1BatchHeader, MiniblockHeader},
    commitment::{L1BatchCommitment, L1BatchMetadata},
    get_code_key, get_system_context_init_logs,
    protocol_version::{L1VerifierConfig, ProtocolVersion},
    tokens::{TokenInfo, TokenMetadata, ETHEREUM_ADDRESS},
    zkevm_test_harness::witness::sort_storage_access::sort_storage_access_queries,
    AccountTreeId, Address, L1BatchNumber, L2ChainId, LogQuery, MiniblockNumber, ProtocolVersionId,
    StorageKey, StorageLog, StorageLogKind, Timestamp, H256,
};
use zksync_utils::{be_words_to_bytes, h256_to_u256};
use zksync_utils::{bytecode::hash_bytecode, u256_to_h256};

use crate::metadata_calculator::L1BatchWithLogs;

#[derive(Debug, Clone)]
pub struct GenesisParams {
    pub first_validator: Address,
    pub protocol_version: ProtocolVersionId,
    pub base_system_contracts: BaseSystemContracts,
    pub system_contracts: Vec<DeployedContract>,
    pub first_verifier_address: Address,
    pub first_l1_verifier_config: L1VerifierConfig,
}

impl GenesisParams {
    #[cfg(test)]
    pub(crate) fn mock() -> Self {
        use zksync_types::system_contracts::get_system_smart_contracts;

        Self {
            first_validator: Address::repeat_byte(0x01),
            protocol_version: ProtocolVersionId::latest(),
            base_system_contracts: BaseSystemContracts::load_from_disk(),
            system_contracts: get_system_smart_contracts(),
            first_l1_verifier_config: L1VerifierConfig::default(),
            first_verifier_address: Address::zero(),
        }
    }
}

pub async fn ensure_genesis_state(
    storage: &mut StorageProcessor<'_>,
    zksync_chain_id: L2ChainId,
    genesis_params: &GenesisParams,
) -> anyhow::Result<H256> {
    let mut transaction = storage.start_transaction().await.unwrap();

    // return if genesis block was already processed
    if !transaction.blocks_dal().is_genesis_needed().await.unwrap() {
        tracing::debug!("genesis is not needed!");
        return transaction
            .blocks_dal()
            .get_l1_batch_state_root(L1BatchNumber(0))
            .await
            .unwrap()
            .context("genesis block hash is empty");
    }

    tracing::info!("running regenesis");
    let GenesisParams {
        first_validator,
        protocol_version,
        base_system_contracts,
        system_contracts,
        first_verifier_address,
        first_l1_verifier_config,
    } = genesis_params;

    let base_system_contracts_hashes = base_system_contracts.hashes();

    create_genesis_l1_batch(
        &mut transaction,
        *first_validator,
        zksync_chain_id,
        *protocol_version,
        base_system_contracts,
        system_contracts,
        *first_l1_verifier_config,
        *first_verifier_address,
    )
    .await;
    tracing::info!("chain_schema_genesis is complete");

    let storage_logs = L1BatchWithLogs::new(&mut transaction, L1BatchNumber(0)).await;
    let storage_logs = storage_logs.unwrap().storage_logs;
    let metadata = ZkSyncTree::process_genesis_batch(&storage_logs);
    let genesis_root_hash = metadata.root_hash;
    let rollup_last_leaf_index = metadata.leaf_count + 1;

    let block_commitment = L1BatchCommitment::new(
        vec![],
        rollup_last_leaf_index,
        genesis_root_hash,
        vec![],
        vec![],
        base_system_contracts_hashes.bootloader,
        base_system_contracts_hashes.default_aa,
        vec![],
        vec![],
        H256::zero(),
        H256::zero(),
        protocol_version.is_pre_boojum(),
    );

    save_genesis_l1_batch_metadata(
        &mut transaction,
        &block_commitment,
        genesis_root_hash,
        rollup_last_leaf_index,
    )
    .await;
    tracing::info!("operations_schema_genesis is complete");

    transaction.commit().await.unwrap();

    // We need to `println` this value because it will be used to initialize the smart contract.
    println!("CONTRACTS_GENESIS_ROOT={:?}", genesis_root_hash);
    println!(
        "CONTRACTS_GENESIS_BATCH_COMMITMENT={:?}",
        block_commitment.hash().commitment
    );
    println!(
        "CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX={}",
        rollup_last_leaf_index
    );
    println!(
        "CHAIN_STATE_KEEPER_BOOTLOADER_HASH={:?}",
        base_system_contracts_hashes.bootloader
    );
    println!(
        "CHAIN_STATE_KEEPER_DEFAULT_AA_HASH={:?}",
        base_system_contracts_hashes.default_aa
    );

    Ok(genesis_root_hash)
}

// Default account and bootloader are not a regular system contracts
// they have never been actually deployed anywhere,
// They are the initial code that is fed into the VM upon its start.
// Both are rather parameters of a block and not system contracts.
// The code of the bootloader should not be deployed anywhere anywhere in the kernel space (i.e. addresses below 2^16)
// because in this case we will have to worry about protecting it.
async fn insert_base_system_contracts_to_factory_deps(
    storage: &mut StorageProcessor<'_>,
    contracts: &BaseSystemContracts,
) {
    let factory_deps = [&contracts.bootloader, &contracts.default_aa]
        .iter()
        .map(|c| (c.hash, be_words_to_bytes(&c.code)))
        .collect();

    storage
        .storage_dal()
        .insert_factory_deps(MiniblockNumber(0), &factory_deps)
        .await;
}

async fn insert_system_contracts(
    storage: &mut StorageProcessor<'_>,
    contracts: &[DeployedContract],
    chain_id: L2ChainId,
) {
    let system_context_init_logs = (H256::default(), get_system_context_init_logs(chain_id));

    let storage_logs: Vec<(H256, Vec<StorageLog>)> = contracts
        .iter()
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

    let mut transaction = storage.start_transaction().await.unwrap();

    transaction
        .storage_logs_dal()
        .insert_storage_logs(MiniblockNumber(0), &storage_logs)
        .await;

    // we don't produce proof for the genesis block,
    // but we still need to populate the table
    // to have the correct initial state of the merkle tree
    let log_queries: Vec<LogQuery> = storage_logs
        .iter()
        .enumerate()
        .flat_map(|(tx_index, (_, storage_logs))| {
            storage_logs
                .iter()
                .enumerate()
                .map(move |(log_index, storage_log)| {
                    LogQuery {
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
                    }
                })
                .collect::<Vec<LogQuery>>()
        })
        .collect();

    let (_, deduped_log_queries) = sort_storage_access_queries(&log_queries);

    let (deduplicated_writes, protective_reads): (Vec<_>, Vec<_>) = deduped_log_queries
        .into_iter()
        .partition(|log_query| log_query.rw_flag);
    transaction
        .storage_logs_dedup_dal()
        .insert_protective_reads(L1BatchNumber(0), &protective_reads)
        .await;

    let written_storage_keys: Vec<_> = deduplicated_writes
        .iter()
        .map(|log| StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key)))
        .collect();
    transaction
        .storage_logs_dedup_dal()
        .insert_initial_writes(L1BatchNumber(0), &written_storage_keys)
        .await;

    transaction
        .storage_dal()
        .apply_storage_logs(&storage_logs)
        .await;

    let factory_deps = contracts
        .iter()
        .map(|c| (hash_bytecode(&c.bytecode), c.bytecode.clone()))
        .collect();
    transaction
        .storage_dal()
        .insert_factory_deps(MiniblockNumber(0), &factory_deps)
        .await;

    transaction.commit().await.unwrap();
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_genesis_l1_batch(
    storage: &mut StorageProcessor<'_>,
    first_validator_address: Address,
    chain_id: L2ChainId,
    protocol_version: ProtocolVersionId,
    base_system_contracts: &BaseSystemContracts,
    system_contracts: &[DeployedContract],
    l1_verifier_config: L1VerifierConfig,
    verifier_address: Address,
) {
    let version = ProtocolVersion {
        id: protocol_version,
        timestamp: 0,
        l1_verifier_config,
        base_system_contracts_hashes: base_system_contracts.hashes(),
        verifier_address,
        tx: None,
    };

    let mut genesis_l1_batch_header = L1BatchHeader::new(
        L1BatchNumber(0),
        0,
        first_validator_address,
        base_system_contracts.hashes(),
        ProtocolVersionId::latest(),
    );
    genesis_l1_batch_header.is_finished = true;

    let genesis_miniblock_header = MiniblockHeader {
        number: MiniblockNumber(0),
        timestamp: 0,
        hash: legacy_miniblock_hash(MiniblockNumber(0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 0,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        base_system_contracts_hashes: base_system_contracts.hashes(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 0,
    };

    let mut transaction = storage.start_transaction().await.unwrap();

    transaction
        .protocol_versions_dal()
        .save_protocol_version_with_tx(version)
        .await;
    transaction
        .blocks_dal()
        .insert_l1_batch(
            &genesis_l1_batch_header,
            &[],
            BlockGasCount::default(),
            &[],
            &[],
        )
        .await
        .unwrap();
    transaction
        .blocks_dal()
        .insert_miniblock(&genesis_miniblock_header)
        .await
        .unwrap();
    transaction
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(0))
        .await
        .unwrap();

    insert_base_system_contracts_to_factory_deps(&mut transaction, base_system_contracts).await;
    insert_system_contracts(&mut transaction, system_contracts, chain_id).await;

    add_eth_token(&mut transaction).await;

    transaction.commit().await.unwrap();
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

    let mut transaction = storage.start_transaction().await.unwrap();

    transaction
        .tokens_dal()
        .add_tokens(vec![eth_token.clone()])
        .await;
    transaction
        .tokens_dal()
        .update_well_known_l1_token(&ETHEREUM_ADDRESS, eth_token.metadata)
        .await;

    transaction.commit().await.unwrap();
}

pub(crate) async fn save_genesis_l1_batch_metadata(
    storage: &mut StorageProcessor<'_>,
    commitment: &L1BatchCommitment,
    genesis_root_hash: H256,
    rollup_last_leaf_index: u64,
) {
    let commitment_hash = commitment.hash();

    let metadata = L1BatchMetadata {
        root_hash: genesis_root_hash,
        rollup_last_leaf_index,
        merkle_root_hash: genesis_root_hash,
        initial_writes_compressed: vec![],
        repeated_writes_compressed: vec![],
        commitment: commitment_hash.commitment,
        l2_l1_messages_compressed: vec![],
        l2_l1_merkle_root: Default::default(),
        block_meta_params: commitment.meta_parameters(),
        aux_data_hash: commitment_hash.aux_output,
        meta_parameters_hash: commitment_hash.meta_parameters,
        pass_through_data_hash: commitment_hash.pass_through_data,
        events_queue_commitment: None,
        bootloader_initial_content_commitment: None,
        state_diffs_compressed: vec![],
    };
    storage
        .blocks_dal()
        .save_genesis_l1_batch_metadata(&metadata)
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use zksync_dal::ConnectionPool;
    use zksync_types::system_contracts::get_system_smart_contracts;

    use super::*;

    #[tokio::test]
    async fn running_genesis() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        conn.blocks_dal().delete_genesis().await.unwrap();

        let params = GenesisParams {
            protocol_version: ProtocolVersionId::latest(),
            first_validator: Address::random(),
            base_system_contracts: BaseSystemContracts::load_from_disk(),
            system_contracts: get_system_smart_contracts(),
            first_l1_verifier_config: L1VerifierConfig::default(),
            first_verifier_address: Address::random(),
        };
        ensure_genesis_state(&mut conn, L2ChainId::from(270), &params)
            .await
            .unwrap();

        assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
        let metadata = conn
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(0))
            .await
            .unwrap();
        let root_hash = metadata.unwrap().metadata.root_hash;
        assert_ne!(root_hash, H256::zero());

        // Check that `ensure_genesis_state()` doesn't panic on repeated runs.
        ensure_genesis_state(&mut conn, L2ChainId::from(270), &params)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn running_genesis_with_big_chain_id() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn: StorageProcessor<'_> = pool.access_storage().await.unwrap();
        conn.blocks_dal().delete_genesis().await.unwrap();

        let params = GenesisParams {
            protocol_version: ProtocolVersionId::latest(),
            first_validator: Address::random(),
            base_system_contracts: BaseSystemContracts::load_from_disk(),
            system_contracts: get_system_smart_contracts(),
            first_l1_verifier_config: L1VerifierConfig::default(),
            first_verifier_address: Address::random(),
        };
        ensure_genesis_state(&mut conn, L2ChainId::max(), &params)
            .await
            .unwrap();

        assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
        let metadata = conn
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(0))
            .await;
        let root_hash = metadata.unwrap().unwrap().metadata.root_hash;
        assert_ne!(root_hash, H256::zero());
    }
}
