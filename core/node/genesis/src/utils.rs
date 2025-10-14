use std::collections::HashMap;

use itertools::Itertools;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_multivm::{
    circuit_sequencer_api_latest::sort_storage_access::sort_storage_access_queries,
    zk_evm_latest::aux_structures::{LogQuery as MultiVmLogQuery, Timestamp as MultiVmTimestamp},
};
use zksync_system_constants::{DEFAULT_ERA_CHAIN_ID, ETHEREUM_ADDRESS};
use zksync_types::{
    block::{DeployedContract, L1BatchTreeData},
    bytecode::BytecodeHash,
    commitment::L1BatchCommitment,
    get_code_key, get_known_code_key, get_system_context_init_logs, h256_to_u256,
    tokens::{TokenInfo, TokenMetadata},
    u256_to_h256,
    zk_evm_types::{LogQuery, Timestamp},
    AccountTreeId, L1BatchNumber, L2BlockNumber, L2ChainId, StorageKey, StorageLog, H256,
};

use crate::GenesisError;

pub(super) async fn add_eth_token(transaction: &mut Connection<'_, Core>) -> anyhow::Result<()> {
    assert!(transaction.in_transaction()); // sanity check
    let eth_token = TokenInfo {
        l1_address: ETHEREUM_ADDRESS,
        l2_address: ETHEREUM_ADDRESS,
        metadata: TokenMetadata {
            name: "Ether".to_string(),
            symbol: "ETH".to_string(),
            decimals: 18,
        },
    };

    transaction.tokens_dal().add_tokens(&[eth_token]).await?;
    Ok(())
}

pub fn get_storage_logs(system_contracts: &[DeployedContract]) -> Vec<StorageLog> {
    let system_context_init_logs =
        // During the genesis all chains have the same id.
        // TODO(EVM-579): make sure that the logic is compatible with Era.
        get_system_context_init_logs(L2ChainId::from(DEFAULT_ERA_CHAIN_ID))
    ;

    let known_code_storage_logs: Vec<_> = system_contracts
        .iter()
        .map(|contract| {
            let hash = BytecodeHash::for_bytecode(&contract.bytecode).value();
            let known_code_key = get_known_code_key(&hash);
            let marked_known_value = H256::from_low_u64_be(1u64);

            StorageLog::new_write_log(known_code_key, marked_known_value)
        })
        .dedup_by(|a, b| a == b)
        .collect();

    let storage_logs: Vec<_> = system_contracts
        .iter()
        .map(|contract| {
            let hash = BytecodeHash::for_bytecode(&contract.bytecode).value();
            let code_key = get_code_key(contract.account_id.address());
            StorageLog::new_write_log(code_key, hash)
        })
        .chain(system_context_init_logs)
        .chain(known_code_storage_logs)
        .collect();

    storage_logs
}

pub fn get_deduped_log_queries(storage_logs: &[StorageLog]) -> Vec<LogQuery> {
    // we don't produce proof for the genesis block,
    // but we still need to populate the table
    // to have the correct initial state of the merkle tree
    let log_queries: Vec<MultiVmLogQuery> = storage_logs
        .iter()
        .map(move |storage_log| {
            MultiVmLogQuery {
                // Timestamp and `tx_number` in block don't matter.
                // `sort_storage_access_queries` assumes that the queries are in chronological order.
                timestamp: MultiVmTimestamp(0),
                tx_number_in_block: 0,
                aux_byte: 0,
                shard_id: 0,
                address: *storage_log.key.address(),
                key: h256_to_u256(*storage_log.key.key()),
                read_value: h256_to_u256(H256::zero()),
                written_value: h256_to_u256(storage_log.value),
                rw_flag: storage_log.is_write(),
                rollback: false,
                is_service: false,
            }
        })
        .collect();

    let deduped_log_queries: Vec<LogQuery> = sort_storage_access_queries(log_queries)
        .1
        .into_iter()
        .map(|log_query| LogQuery {
            timestamp: Timestamp(log_query.timestamp.0),
            tx_number_in_block: log_query.tx_number_in_block,
            aux_byte: log_query.aux_byte,
            shard_id: log_query.shard_id,
            address: log_query.address,
            key: log_query.key,
            read_value: log_query.read_value,
            written_value: log_query.written_value,
            rw_flag: log_query.rw_flag,
            rollback: log_query.rollback,
            is_service: log_query.is_service,
        })
        .collect();

    deduped_log_queries
}

/// Default account, bootloader and EVM emulator are not regular system contracts.
/// They have never been actually deployed anywhere, rather, they are the initial code that is fed into the VM upon its start.
/// Hence, they are rather parameters of a block and not *real* system contracts.
/// The code of the bootloader should not be deployed anywhere in the kernel space (i.e. addresses below 2^16)
/// because in this case we will have to worry about protecting it.
pub(super) async fn insert_base_system_contracts_to_factory_deps(
    storage: &mut Connection<'_, Core>,
    contracts: &BaseSystemContracts,
) -> Result<(), GenesisError> {
    let factory_deps = [&contracts.bootloader, &contracts.default_aa]
        .into_iter()
        .chain(contracts.evm_emulator.as_ref())
        .map(|c| (c.hash, c.code.clone()))
        .collect();

    Ok(storage
        .factory_deps_dal()
        .insert_factory_deps(L2BlockNumber(0), &factory_deps)
        .await?)
}

pub(super) async fn save_genesis_l1_batch_metadata(
    storage: &mut Connection<'_, Core>,
    commitment: L1BatchCommitment,
    genesis_root_hash: H256,
    rollup_last_leaf_index: u64,
) -> Result<(), GenesisError> {
    let mut transaction = storage.start_transaction().await?;

    let tree_data = L1BatchTreeData {
        hash: genesis_root_hash,
        rollup_last_leaf_index,
    };
    transaction
        .blocks_dal()
        .save_l1_batch_tree_data(L1BatchNumber(0), &tree_data)
        .await?;

    let mut commitment_artifacts = commitment.artifacts()?;
    // `l2_l1_merkle_root` for genesis batch is set to 0 on L1 contract, same must be here.
    commitment_artifacts.l2_l1_merkle_root = H256::zero();

    transaction
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(L1BatchNumber(0), &commitment_artifacts)
        .await?;

    transaction.commit().await?;
    Ok(())
}

pub(super) async fn insert_storage_logs(
    transaction: &mut Connection<'_, Core>,
    storage_logs: &[StorageLog],
) -> Result<(), GenesisError> {
    transaction
        .storage_logs_dal()
        .insert_storage_logs(L2BlockNumber(0), storage_logs)
        .await?;
    Ok(())
}

pub(super) async fn insert_deduplicated_writes_and_protective_reads(
    transaction: &mut Connection<'_, Core>,
    deduped_log_queries: &[LogQuery],
) -> Result<(), GenesisError> {
    let (deduplicated_writes, protective_reads): (Vec<_>, Vec<_>) = deduped_log_queries
        .iter()
        .partition(|log_query| log_query.rw_flag);

    transaction
        .storage_logs_dedup_dal()
        .insert_protective_reads(
            L1BatchNumber(0),
            &protective_reads
                .iter()
                .map(|log_query: &LogQuery| StorageLog::from(*log_query)) // Pass the log_query to from()
                .collect::<Vec<_>>(),
        )
        .await?;

    let written_storage_keys: Vec<_> = deduplicated_writes
        .iter()
        .map(|log| {
            StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key)).hashed_key()
        })
        .collect();

    transaction
        .storage_logs_dedup_dal()
        .insert_initial_writes(L1BatchNumber(0), &written_storage_keys)
        .await?;

    Ok(())
}

pub(super) async fn insert_factory_deps(
    transaction: &mut Connection<'_, Core>,
    factory_deps: HashMap<H256, Vec<u8>>,
) -> Result<(), GenesisError> {
    transaction
        .factory_deps_dal()
        .insert_factory_deps(L2BlockNumber(0), &factory_deps)
        .await?;
    Ok(())
}
