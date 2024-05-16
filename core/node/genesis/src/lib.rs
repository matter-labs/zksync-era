//! This module aims to provide a genesis setup for the zkSync Era network.
//! It initializes the Merkle tree with the basic setup (such as fields of special service accounts),
//! setups the required databases, and outputs the data required to initialize a smart contract.

use std::fmt::Formatter;

use anyhow::Context as _;
use multivm::utils::get_max_gas_per_pubdata_byte;
use zksync_config::{GenesisConfig, PostgresConfig};
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SET_CHAIN_ID_EVENT};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError};
use zksync_eth_client::EthInterface;
use zksync_merkle_tree::{domain::ZkSyncTree, TreeInstruction};
use zksync_system_constants::PRIORITY_EXPIRATION;
use zksync_types::{
    block::{BlockGasCount, DeployedContract, L1BatchHeader, L2BlockHasher, L2BlockHeader},
    commitment::{CommitmentInput, L1BatchCommitment},
    fee_model::BatchFeeInput,
    protocol_upgrade::decode_set_chain_id_event,
    protocol_version::{L1VerifierConfig, VerifierParams},
    system_contracts::get_system_smart_contracts,
    web3::{BlockNumber, FilterBuilder},
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersion,
    ProtocolVersionId, StorageKey, H256,
};
use zksync_utils::{bytecode::hash_bytecode, u256_to_h256};

use crate::utils::{
    add_eth_token, get_deduped_log_queries, get_storage_logs,
    insert_base_system_contracts_to_factory_deps, insert_system_contracts,
    save_genesis_l1_batch_metadata,
};

#[cfg(test)]
mod tests;
mod utils;

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
    DBError(#[from] DalError),
    #[error("Error: {0}")]
    Other(#[from] anyhow::Error),
    #[error("Field: {0} required for genesis")]
    MalformedConfig(&'static str),
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
            bootloader: config
                .bootloader_hash
                .ok_or(GenesisError::MalformedConfig("bootloader_hash"))?,
            default_aa: config
                .default_aa_hash
                .ok_or(GenesisError::MalformedConfig("default_aa_hash"))?,
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
            .ok_or(GenesisError::MalformedConfig("protocol_version"))?
            .try_into()
            .map_err(|_| GenesisError::ProtocolVersion(config.protocol_version.unwrap()))?;
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

    pub fn mock() -> Self {
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
            .expect("Protocol version must be set")
            .try_into()
            .expect("Protocol version must be correctly initialized for genesis")
    }
}

pub struct GenesisBatchParams {
    pub root_hash: H256,
    pub commitment: H256,
    pub rollup_last_leaf_index: u64,
}

pub fn mock_genesis_config() -> GenesisConfig {
    use zksync_types::L1ChainId;

    let base_system_contracts_hashes = BaseSystemContracts::load_from_disk().hashes();
    let first_l1_verifier_config = L1VerifierConfig::default();

    GenesisConfig {
        protocol_version: Some(ProtocolVersionId::latest() as u16),
        genesis_root_hash: Some(H256::default()),
        rollup_last_leaf_index: Some(26),
        genesis_commitment: Some(H256::default()),
        bootloader_hash: Some(base_system_contracts_hashes.bootloader),
        default_aa_hash: Some(base_system_contracts_hashes.default_aa),
        l1_chain_id: L1ChainId(9),
        l2_chain_id: L2ChainId::default(),
        recursion_node_level_vk_hash: first_l1_verifier_config.params.recursion_node_level_vk_hash,
        recursion_leaf_level_vk_hash: first_l1_verifier_config.params.recursion_leaf_level_vk_hash,
        recursion_circuits_set_vks_hash: first_l1_verifier_config
            .params
            .recursion_circuits_set_vks_hash,
        recursion_scheduler_level_vk_hash: first_l1_verifier_config
            .recursion_scheduler_level_vk_hash,
        fee_account: Default::default(),
        dummy_verifier: false,
        l1_batch_commit_data_generator_mode: Default::default(),
    }
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
        genesis_params.protocol_version(),
        genesis_params.base_system_contracts(),
        genesis_params.system_contracts(),
        verifier_config,
    )
    .await?;
    tracing::info!("chain_schema_genesis is complete");

    let deduped_log_queries =
        get_deduped_log_queries(&get_storage_logs(genesis_params.system_contracts()));

    let (deduplicated_writes, _): (Vec<_>, Vec<_>) = deduped_log_queries
        .into_iter()
        .partition(|log_query| log_query.rw_flag);

    let storage_logs: Vec<TreeInstruction<StorageKey>> = deduplicated_writes
        .iter()
        .enumerate()
        .map(|(index, log)| {
            TreeInstruction::write(
                StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key)),
                (index + 1) as u64,
                u256_to_h256(log.written_value),
            )
        })
        .collect();

    let metadata = ZkSyncTree::process_genesis_batch(&storage_logs);
    let genesis_root_hash = metadata.root_hash;
    let rollup_last_leaf_index = metadata.leaf_count + 1;

    let base_system_contract_hashes = BaseSystemContractsHashes {
        bootloader: genesis_params
            .config
            .bootloader_hash
            .ok_or(GenesisError::MalformedConfig("bootloader"))?,
        default_aa: genesis_params
            .config
            .default_aa_hash
            .ok_or(GenesisError::MalformedConfig("default_aa_hash"))?,
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

    let expected_root_hash = genesis_params
        .config
        .genesis_root_hash
        .ok_or(GenesisError::MalformedConfig("genesis_root_hash"))?;
    let expected_commitment = genesis_params
        .config
        .genesis_commitment
        .ok_or(GenesisError::MalformedConfig("expected_commitment"))?;
    let expected_rollup_last_leaf_index =
        genesis_params
            .config
            .rollup_last_leaf_index
            .ok_or(GenesisError::MalformedConfig(
                "expected_rollup_last_leaf_index",
            ))?;

    if expected_root_hash != root_hash {
        return Err(GenesisError::RootHash(expected_root_hash, root_hash));
    }

    if expected_commitment != commitment {
        return Err(GenesisError::Commitment(expected_commitment, commitment));
    }

    if expected_rollup_last_leaf_index != rollup_last_leaf_index {
        return Err(GenesisError::LeafIndexes(
            expected_rollup_last_leaf_index,
            rollup_last_leaf_index,
        ));
    }

    tracing::info!("genesis is complete");
    transaction.commit().await?;
    Ok(root_hash)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_genesis_l1_batch(
    storage: &mut Connection<'_, Core>,
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

    let genesis_l2_block_header = L2BlockHeader {
        number: L2BlockNumber(0),
        timestamp: 0,
        hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Default::default(),
        base_fee_per_gas: 0,
        gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(protocol_version.into()),
        batch_fee_input: BatchFeeInput::l1_pegged(0, 0),
        base_system_contracts_hashes: base_system_contracts.hashes(),
        protocol_version: Some(protocol_version),
        virtual_blocks: 0,
        gas_limit: 0,
    };

    let mut transaction = storage.start_transaction().await?;

    transaction
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&version)
        .await?;
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
        .insert_l2_block(&genesis_l2_block_header)
        .await?;
    transaction
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(0))
        .await?;

    let storage_logs = get_storage_logs(system_contracts);

    let factory_deps = system_contracts
        .iter()
        .map(|c| (hash_bytecode(&c.bytecode), c.bytecode.clone()))
        .collect();

    insert_base_system_contracts_to_factory_deps(&mut transaction, base_system_contracts).await?;
    insert_system_contracts(&mut transaction, factory_deps, &storage_logs).await?;
    add_eth_token(&mut transaction).await?;

    transaction.commit().await?;
    Ok(())
}

// Save chain id transaction into the database
// We keep returning anyhow and will refactor it later
pub async fn save_set_chain_id_tx(
    query_client: &dyn EthInterface,
    diamond_proxy_address: Address,
    state_transition_manager_address: Address,
    postgres_config: &PostgresConfig,
) -> anyhow::Result<()> {
    let db_url = postgres_config.master_url()?;
    let pool = ConnectionPool::<Core>::singleton(db_url).build().await?;
    let mut storage = pool.connection().await?;

    let to = query_client.block_number().await?.as_u64();
    let from = to.saturating_sub(PRIORITY_EXPIRATION);
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
    let mut logs = query_client.logs(filter).await?;
    anyhow::ensure!(
        logs.len() == 1,
        "Expected a single set_chain_id event, got these {}: {:?}",
        logs.len(),
        logs
    );
    let (version_id, upgrade_tx) =
        decode_set_chain_id_event(logs.remove(0)).context("Chain id event is incorrect")?;

    tracing::info!("New version id {:?}", version_id);
    storage
        .protocol_versions_dal()
        .save_genesis_upgrade_with_tx(version_id, &upgrade_tx)
        .await?;
    Ok(())
}
