//! This module aims to provide a genesis setup for the zkSync Era network.
//! It initializes the Merkle tree with the basic setup (such as fields of special service accounts),
//! setups the required databases, and outputs the data required to initialize a smart contract.

use std::fmt::Formatter;

use anyhow::Context as _;
use multivm::{
    circuit_sequencer_api_latest::sort_storage_access::sort_storage_access_queries,
    utils::get_max_gas_per_pubdata_byte,
    zk_evm_latest::aux_structures::{LogQuery as MultiVmLogQuery, Timestamp as MultiVMTimestamp},
};
use zksync_config::{GenesisConfig, PostgresConfig};
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SET_CHAIN_ID_EVENT};
use zksync_dal::{Connection, Core, CoreDal, SqlxError};
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_eth_client::{clients::QueryClient, EthInterface};
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_system_constants::PRIORITY_EXPIRATION;
use zksync_types::{
    block::{
        BlockGasCount, DeployedContract, L1BatchHeader, L1BatchTreeData, MiniblockHasher,
        MiniblockHeader,
    },
    commitment::{CommitmentInput, L1BatchCommitment},
    fee_model::BatchFeeInput,
    get_code_key, get_system_context_init_logs,
    protocol_upgrade::{decode_set_chain_id_event, ProtocolVersion},
    protocol_version::{L1VerifierConfig, VerifierParams},
    system_contracts::get_system_smart_contracts,
    tokens::{TokenInfo, TokenMetadata, ETHEREUM_ADDRESS},
    web3::types::{BlockNumber, FilterBuilder},
    zk_evm_types::{LogQuery, Timestamp},
    AccountTreeId, Address, L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId,
    StorageKey, StorageLog, StorageLogKind, H256,
};
use zksync_utils::{be_words_to_bytes, bytecode::hash_bytecode, h256_to_u256, u256_to_h256};

use crate::metadata_calculator::L1BatchWithLogs;

#[derive(Debug, Clone)]
pub struct BaseContractsHashError {
    from_config: BaseSystemContractsHashes,
    calculated: BaseSystemContractsHashes,
}

impl std::fmt::Display for BaseContractsHashError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "From Config {:?}, Calculated : {:?}",
            &self.from_config, &self.calculated
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GenesisError {
    #[error("Root hash mismatched: From config: {0:?}, Calculated {1:?}")]
    RootHash(H256, H256),
    #[error("Leaf indexes mismatched: From config: {0:?}, Calculated {1:?}")]
    LeafIndexes(u64, u64),
    #[error("Base system contracts mismatched: {0}")]
    BaseSystemContractsHashes(Box<BaseContractsHashError>),
    #[error("Commitment mismatched: From config: {0:?}, Calculated {1:?}")]
    Commitment(H256, H256),
    #[error("Wrong protocol version")]
    ProtocolVersion(u16),
    #[error("DB Error: {0}")]
    DBError(#[from] SqlxError),
    #[error("Error: {0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone)]
pub struct GenesisParams {
    base_system_contracts: BaseSystemContracts,
    system_contracts: Vec<DeployedContract>,
    config: GenesisConfig,
}

impl GenesisParams {
    pub fn system_contracts(&self) -> &[DeployedContract] {
        &self.system_contracts
    }
    pub fn base_system_contracts(&self) -> &BaseSystemContracts {
        &self.base_system_contracts
    }
    pub fn config(&self) -> &GenesisConfig {
        &self.config
    }

    pub fn from_genesis_config(
        config: GenesisConfig,
        base_system_contracts: BaseSystemContracts,
        system_contracts: Vec<DeployedContract>,
    ) -> Result<GenesisParams, GenesisError> {
        let base_system_contracts_hashes = BaseSystemContractsHashes {
            bootloader: config.bootloader_hash,
            default_aa: config.default_aa_hash,
        };
        if base_system_contracts_hashes != base_system_contracts.hashes() {
            return Err(GenesisError::BaseSystemContractsHashes(Box::new(
                BaseContractsHashError {
                    from_config: base_system_contracts_hashes,
                    calculated: base_system_contracts.hashes(),
                },
            )));
        }
        // Try to convert value from config to the real protocol version and return error
        // if the version doesn't exist
        let _: ProtocolVersionId = config
            .protocol_version
            .try_into()
            .map_err(|_| GenesisError::ProtocolVersion(config.protocol_version))?;
        Ok(GenesisParams {
            base_system_contracts,
            system_contracts,
            config,
        })
    }

    pub fn load_genesis_params(config: GenesisConfig) -> Result<GenesisParams, GenesisError> {
        let base_system_contracts = BaseSystemContracts::load_from_disk();
        let system_contracts = get_system_smart_contracts();
        Self::from_genesis_config(config, base_system_contracts, system_contracts)
    }

    #[cfg(test)]
    pub(crate) fn mock() -> Self {
        Self {
            base_system_contracts: BaseSystemContracts::load_from_disk(),
            system_contracts: get_system_smart_contracts(),
            config: mock_genesis_config(),
        }
    }

    pub fn protocol_version(&self) -> ProtocolVersionId {
        // It's impossible to instantiate Genesis params with wrong protocol version
        self.config
            .protocol_version
            .try_into()
            .expect("Protocol version must be correctly initialized for genesis")
    }
}

#[cfg(test)]
pub fn mock_genesis_config() -> GenesisConfig {
    use zksync_types::L1ChainId;

    let base_system_contracts_hashes = BaseSystemContracts::load_from_disk().hashes();
    let first_l1_verifier_config = L1VerifierConfig::default();

    GenesisConfig {
        protocol_version: ProtocolVersionId::latest() as u16,
        genesis_root_hash: Default::default(),
        rollup_last_leaf_index: 26,
        genesis_commitment: Default::default(),
        bootloader_hash: base_system_contracts_hashes.bootloader,
        default_aa_hash: base_system_contracts_hashes.default_aa,
        fee_account: Address::repeat_byte(0x01),
        l1_chain_id: L1ChainId(9),
        l2_chain_id: L2ChainId::default(),
        recursion_node_level_vk_hash: first_l1_verifier_config.params.recursion_node_level_vk_hash,
        recursion_leaf_level_vk_hash: first_l1_verifier_config.params.recursion_leaf_level_vk_hash,
        recursion_scheduler_level_vk_hash: first_l1_verifier_config
            .recursion_scheduler_level_vk_hash,
    }
}

pub struct GenesisBatchParams {
    pub root_hash: H256,
    pub commitment: H256,
    pub rollup_last_leaf_index: u64,
}

// Insert genesis batch into the database
pub async fn insert_genesis_batch(
    storage: &mut Connection<'_, Core>,
    genesis_params: &GenesisParams,
) -> Result<GenesisBatchParams, GenesisError> {
    let mut transaction = storage.start_transaction().await?;
    let verifier_config = L1VerifierConfig {
        params: VerifierParams {
            recursion_node_level_vk_hash: genesis_params.config.recursion_node_level_vk_hash,
            recursion_leaf_level_vk_hash: genesis_params.config.recursion_leaf_level_vk_hash,
            recursion_circuits_set_vks_hash: H256::zero(),
        },
        recursion_scheduler_level_vk_hash: genesis_params.config.recursion_scheduler_level_vk_hash,
    };

    create_genesis_l1_batch(
        &mut transaction,
        genesis_params.config.fee_account,
        genesis_params.config.l2_chain_id,
        genesis_params.protocol_version(),
        genesis_params.base_system_contracts(),
        genesis_params.system_contracts(),
        verifier_config,
    )
    .await?;
    tracing::info!("chain_schema_genesis is complete");

    let storage_logs = L1BatchWithLogs::new(&mut transaction, L1BatchNumber(0)).await;
    let storage_logs = storage_logs
        .context("genesis L1 batch disappeared from Postgres")?
        .storage_logs;
    let metadata = ZkSyncTree::process_genesis_batch(&storage_logs);
    let genesis_root_hash = metadata.root_hash;
    let rollup_last_leaf_index = metadata.leaf_count + 1;

    let base_system_contract_hashes = BaseSystemContractsHashes {
        bootloader: genesis_params.config.bootloader_hash,
        default_aa: genesis_params.config.default_aa_hash,
    };
    let commitment_input = CommitmentInput::for_genesis_batch(
        genesis_root_hash,
        rollup_last_leaf_index,
        base_system_contract_hashes,
        genesis_params.protocol_version(),
    );
    let block_commitment = L1BatchCommitment::new(commitment_input);

    save_genesis_l1_batch_metadata(
        &mut transaction,
        block_commitment.clone(),
        genesis_root_hash,
        rollup_last_leaf_index,
    )
    .await?;
    transaction.commit().await?;
    Ok(GenesisBatchParams {
        root_hash: genesis_root_hash,
        commitment: block_commitment.hash().commitment,
        rollup_last_leaf_index,
    })
}

pub async fn ensure_genesis_state(
    storage: &mut Connection<'_, Core>,
    genesis_params: &GenesisParams,
) -> Result<H256, GenesisError> {
    let mut transaction = storage.start_transaction().await?;

    if !transaction.blocks_dal().is_genesis_needed().await? {
        tracing::debug!("genesis is not needed!");
        return Ok(transaction
            .blocks_dal()
            .get_l1_batch_state_root(L1BatchNumber(0))
            .await?
            .context("genesis L1 batch hash is empty")?);
    }

    tracing::info!("running regenesis");
    let GenesisBatchParams {
        root_hash,
        commitment,
        rollup_last_leaf_index,
    } = insert_genesis_batch(&mut transaction, genesis_params).await?;
    if genesis_params.config.genesis_root_hash != root_hash {
        return Err(GenesisError::RootHash(
            genesis_params.config.genesis_root_hash,
            root_hash,
        ));
    }

    if genesis_params.config.genesis_commitment != commitment {
        return Err(GenesisError::Commitment(
            genesis_params.config.genesis_commitment,
            commitment,
        ));
    }

    if genesis_params.config.rollup_last_leaf_index != rollup_last_leaf_index {
        return Err(GenesisError::LeafIndexes(
            genesis_params.config.rollup_last_leaf_index,
            rollup_last_leaf_index,
        ));
    }

    tracing::info!("genesis is complete");
    transaction.commit().await?;
    Ok(root_hash)
}

// Default account and bootloader are not a regular system contracts
// they have never been actually deployed anywhere,
// They are the initial code that is fed into the VM upon its start.
// Both are rather parameters of a block and not system contracts.
// The code of the bootloader should not be deployed anywhere anywhere in the kernel space (i.e. addresses below 2^16)
// because in this case we will have to worry about protecting it.
async fn insert_base_system_contracts_to_factory_deps(
    storage: &mut Connection<'_, Core>,
    contracts: &BaseSystemContracts,
) -> Result<(), GenesisError> {
    let factory_deps = [&contracts.bootloader, &contracts.default_aa]
        .iter()
        .map(|c| (c.hash, be_words_to_bytes(&c.code)))
        .collect();

    Ok(storage
        .factory_deps_dal()
        .insert_factory_deps(MiniblockNumber(0), &factory_deps)
        .await?)
}

async fn insert_system_contracts(
    storage: &mut Connection<'_, Core>,
    contracts: &[DeployedContract],
    chain_id: L2ChainId,
) -> Result<(), GenesisError> {
    let system_context_init_logs = (H256::default(), get_system_context_init_logs(chain_id));

    let storage_logs: Vec<_> = contracts
        .iter()
        .map(|contract| {
            let hash = hash_bytecode(&contract.bytecode);
            let code_key = get_code_key(contract.account_id.address());
            (
                H256::default(),
                vec![StorageLog::new_write_log(code_key, hash)],
            )
        })
        .chain(Some(system_context_init_logs))
        .collect();

    let mut transaction = storage.start_transaction().await?;
    transaction
        .storage_logs_dal()
        .insert_storage_logs(MiniblockNumber(0), &storage_logs)
        .await?;

    // we don't produce proof for the genesis block,
    // but we still need to populate the table
    // to have the correct initial state of the merkle tree
    let log_queries: Vec<MultiVmLogQuery> = storage_logs
        .iter()
        .enumerate()
        .flat_map(|(tx_index, (_, storage_logs))| {
            storage_logs
                .iter()
                .enumerate()
                .map(move |(log_index, storage_log)| {
                    MultiVmLogQuery {
                        // Monotonically increasing Timestamp. Normally it's generated by the VM, but we don't have a VM in the genesis block.
                        timestamp: MultiVMTimestamp(((tx_index << 16) + log_index) as u32),
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
                .collect::<Vec<_>>()
        })
        .collect();

    let deduped_log_queries: Vec<LogQuery> = sort_storage_access_queries(&log_queries)
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

    let (deduplicated_writes, protective_reads): (Vec<_>, Vec<_>) = deduped_log_queries
        .into_iter()
        .partition(|log_query| log_query.rw_flag);
    transaction
        .storage_logs_dedup_dal()
        .insert_protective_reads(L1BatchNumber(0), &protective_reads)
        .await?;

    let written_storage_keys: Vec<_> = deduplicated_writes
        .iter()
        .map(|log| StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key)))
        .collect();
    transaction
        .storage_logs_dedup_dal()
        .insert_initial_writes(L1BatchNumber(0), &written_storage_keys)
        .await?;

    #[allow(deprecated)]
    transaction
        .storage_dal()
        .apply_storage_logs(&storage_logs)
        .await;

    let factory_deps = contracts
        .iter()
        .map(|c| (hash_bytecode(&c.bytecode), c.bytecode.clone()))
        .collect();
    transaction
        .factory_deps_dal()
        .insert_factory_deps(MiniblockNumber(0), &factory_deps)
        .await?;

    transaction.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_genesis_l1_batch(
    storage: &mut Connection<'_, Core>,
    first_validator_address: Address,
    chain_id: L2ChainId,
    protocol_version: ProtocolVersionId,
    base_system_contracts: &BaseSystemContracts,
    system_contracts: &[DeployedContract],
    l1_verifier_config: L1VerifierConfig,
) -> Result<(), GenesisError> {
    let version = ProtocolVersion {
        id: protocol_version,
        timestamp: 0,
        l1_verifier_config,
        base_system_contracts_hashes: base_system_contracts.hashes(),
        tx: None,
    };

    let genesis_l1_batch_header = L1BatchHeader::new(
        L1BatchNumber(0),
        0,
        base_system_contracts.hashes(),
        protocol_version,
    );

    let genesis_miniblock_header = MiniblockHeader {
        number: MiniblockNumber(0),
        timestamp: 0,
        hash: MiniblockHasher::legacy_hash(MiniblockNumber(0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: first_validator_address,
        base_fee_per_gas: 0,
        gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(protocol_version.into()),
        batch_fee_input: BatchFeeInput::l1_pegged(0, 0),
        base_system_contracts_hashes: base_system_contracts.hashes(),
        protocol_version: Some(protocol_version),
        virtual_blocks: 0,
    };

    let mut transaction = storage.start_transaction().await?;

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
            Default::default(),
        )
        .await?;
    transaction
        .blocks_dal()
        .insert_miniblock(&genesis_miniblock_header)
        .await?;
    transaction
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(0))
        .await?;

    insert_base_system_contracts_to_factory_deps(&mut transaction, base_system_contracts).await?;
    insert_system_contracts(&mut transaction, system_contracts, chain_id).await?;
    add_eth_token(&mut transaction).await?;

    transaction.commit().await?;
    Ok(())
}

async fn add_eth_token(transaction: &mut Connection<'_, Core>) -> anyhow::Result<()> {
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
    transaction
        .tokens_dal()
        .mark_token_as_well_known(ETHEREUM_ADDRESS)
        .await?;
    Ok(())
}

async fn save_genesis_l1_batch_metadata(
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

    let mut commitment_artifacts = commitment.artifacts();
    // `l2_l1_merkle_root` for genesis batch is set to 0 on L1 contract, same must be here.
    commitment_artifacts.l2_l1_merkle_root = H256::zero();

    transaction
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(L1BatchNumber(0), &commitment_artifacts)
        .await?;

    transaction.commit().await?;
    Ok(())
}

// Save chain id transaction into the database
// We keep returning anyhow and will refactor it later
pub async fn save_set_chain_id_tx(
    eth_client_url: &str,
    diamond_proxy_address: Address,
    state_transition_manager_address: Address,
    postgres_config: &PostgresConfig,
) -> anyhow::Result<()> {
    let db_url = postgres_config.master_url()?;
    let pool = ConnectionPool::<Core>::singleton(db_url).build().await?;
    let mut storage = pool.connection().await?;

    let eth_client = QueryClient::new(eth_client_url)?;
    let to = eth_client.block_number("fetch_chain_id_tx").await?.as_u64();
    let from = to - PRIORITY_EXPIRATION;
    let filter = FilterBuilder::default()
        .address(vec![state_transition_manager_address])
        .topics(
            Some(vec![SET_CHAIN_ID_EVENT.signature()]),
            Some(vec![diamond_proxy_address.into()]),
            None,
            None,
        )
        .from_block(from.into())
        .to_block(BlockNumber::Latest)
        .build();
    let mut logs = eth_client.logs(filter, "fetch_chain_id_tx").await?;
    anyhow::ensure!(
        logs.len() == 1,
        "Expected a single set_chain_id event, got these {}: {:?}",
        logs.len(),
        logs
    );
    let (version_id, upgrade_tx) =
        decode_set_chain_id_event(logs.remove(0)).context("Chain id event is incorrect")?;
    storage
        .protocol_versions_dal()
        .save_genesis_upgrade_with_tx(version_id, upgrade_tx)
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use zksync_config::GenesisConfig;
    use zksync_dal::{ConnectionPool, Core, CoreDal};

    use super::*;

    #[tokio::test]
    async fn running_genesis() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.blocks_dal().delete_genesis().await.unwrap();

        let params = GenesisParams::mock();

        insert_genesis_batch(&mut conn, &params).await.unwrap();

        assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
        let metadata = conn
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(0))
            .await
            .unwrap();
        let root_hash = metadata.unwrap().metadata.root_hash;
        assert_ne!(root_hash, H256::zero());

        // Check that `genesis is not needed`
        assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
    }

    #[tokio::test]
    async fn running_genesis_with_big_chain_id() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.blocks_dal().delete_genesis().await.unwrap();

        let params = GenesisParams::load_genesis_params(GenesisConfig {
            l2_chain_id: L2ChainId::max(),
            ..mock_genesis_config()
        })
        .unwrap();
        insert_genesis_batch(&mut conn, &params).await.unwrap();

        assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
        let metadata = conn
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(0))
            .await;
        let root_hash = metadata.unwrap().unwrap().metadata.root_hash;
        assert_ne!(root_hash, H256::zero());
    }

    #[tokio::test]
    async fn running_genesis_with_non_latest_protocol_version() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let params = GenesisParams::load_genesis_params(GenesisConfig {
            protocol_version: ProtocolVersionId::Version10 as u16,
            ..mock_genesis_config()
        })
        .unwrap();

        insert_genesis_batch(&mut conn, &params).await.unwrap();
        assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
    }
}
