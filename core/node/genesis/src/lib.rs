//! This module aims to provide a genesis setup for the ZKsync Era network.
//! It initializes the Merkle tree with the basic setup (such as fields of special service accounts),
//! setups the required databases, and outputs the data required to initialize a smart contract.

use std::{collections::HashMap, fmt::Formatter};

use anyhow::Context as _;
use kzg::ZK_SYNC_BYTES_PER_BLOB;
use zksync_config::GenesisConfig;
use zksync_contracts::{
    hyperchain_contract, verifier_contract, BaseSystemContracts, BaseSystemContractsHashes,
    GENESIS_UPGRADE_EVENT,
};
use zksync_dal::{custom_genesis_export_dal::GenesisState, Connection, Core, CoreDal, DalError};
use zksync_eth_client::{CallFunctionArgs, EthInterface};
use zksync_merkle_tree::{domain::ZkSyncTree, TreeInstruction};
use zksync_multivm::utils::get_max_gas_per_pubdata_byte;
use zksync_types::{
    block::{DeployedContract, L1BatchHeader, L2BlockHasher, L2BlockHeader},
    bytecode::BytecodeHash,
    commitment::{CommitmentInput, L1BatchCommitment},
    fee_model::BatchFeeInput,
    protocol_upgrade::decode_genesis_upgrade_event,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    settlement::SettlementLayer,
    system_contracts::get_system_smart_contracts,
    u256_to_h256,
    web3::{BlockNumber, FilterBuilder},
    zk_evm_types::LogQuery,
    AccountTreeId, Address, Bloom, L1BatchNumber, L1ChainId, L2BlockNumber, L2ChainId,
    ProtocolVersion, ProtocolVersionId, SLChainId, StorageKey, StorageLog, H256, U256,
};

use crate::utils::{
    add_eth_token, get_deduped_log_queries, get_storage_logs,
    insert_base_system_contracts_to_factory_deps, insert_deduplicated_writes_and_protective_reads,
    insert_factory_deps, insert_storage_logs, save_genesis_l1_batch_metadata,
};

#[cfg(test)]
mod tests;
pub mod utils;

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
            evm_emulator: config.evm_emulator_hash,
        };
        if base_system_contracts_hashes != base_system_contracts.hashes() {
            return Err(GenesisError::BaseSystemContractsHashes(Box::new(
                BaseContractsHashError {
                    from_config: base_system_contracts_hashes,
                    calculated: base_system_contracts.hashes(),
                },
            )));
        }
        if config.protocol_version.is_none() {
            return Err(GenesisError::MalformedConfig("protocol_version"));
        }
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

    pub fn minor_protocol_version(&self) -> ProtocolVersionId {
        self.config
            .protocol_version
            .expect("Protocol version must be set")
            .minor
    }

    pub fn protocol_version(&self) -> ProtocolSemanticVersion {
        self.config
            .protocol_version
            .expect("Protocol version must be set")
    }
}

#[derive(Debug)]
pub struct GenesisBatchParams {
    pub root_hash: H256,
    pub commitment: H256,
    pub rollup_last_leaf_index: u64,
}

pub fn mock_genesis_config() -> GenesisConfig {
    let base_system_contracts_hashes = BaseSystemContracts::load_from_disk().hashes();
    let first_l1_verifier_config = L1VerifierConfig::default();

    GenesisConfig {
        protocol_version: Some(ProtocolSemanticVersion {
            minor: ProtocolVersionId::latest(),
            patch: 0.into(),
        }),
        genesis_root_hash: Some(H256::default()),
        rollup_last_leaf_index: Some(26),
        genesis_commitment: Some(H256::default()),
        bootloader_hash: Some(base_system_contracts_hashes.bootloader),
        default_aa_hash: Some(base_system_contracts_hashes.default_aa),
        evm_emulator_hash: base_system_contracts_hashes.evm_emulator,
        l1_chain_id: L1ChainId(9),
        l2_chain_id: L2ChainId::default(),
        snark_wrapper_vk_hash: first_l1_verifier_config.snark_wrapper_vk_hash,
        fflonk_snark_wrapper_vk_hash: first_l1_verifier_config.fflonk_snark_wrapper_vk_hash,
        fee_account: Default::default(),
        dummy_verifier: false,
        l1_batch_commit_data_generator_mode: Default::default(),
        custom_genesis_state_path: None,
    }
}

pub fn make_genesis_batch_params(
    deduped_log_queries: Vec<LogQuery>,
    base_system_contract_hashes: BaseSystemContractsHashes,
    protocol_version: ProtocolVersionId,
) -> (GenesisBatchParams, L1BatchCommitment) {
    let storage_logs = deduped_log_queries
        .into_iter()
        .filter(|log_query| log_query.rw_flag) // only writes
        .enumerate()
        .map(|(index, log)| {
            TreeInstruction::write(
                StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key))
                    .hashed_key_u256(),
                (index + 1) as u64,
                u256_to_h256(log.written_value),
            )
        })
        .collect::<Vec<_>>();

    let metadata = ZkSyncTree::process_genesis_batch(&storage_logs);
    let root_hash = metadata.root_hash;
    let rollup_last_leaf_index = metadata.leaf_count + 1;

    let commitment_input = CommitmentInput::for_genesis_batch(
        root_hash,
        rollup_last_leaf_index,
        base_system_contract_hashes,
        protocol_version,
    );
    let block_commitment = L1BatchCommitment::new(commitment_input);
    let commitment = block_commitment.hash().commitment;

    (
        GenesisBatchParams {
            root_hash,
            commitment,
            rollup_last_leaf_index,
        },
        block_commitment,
    )
}

pub async fn insert_genesis_batch_with_custom_state(
    storage: &mut Connection<'_, Core>,
    genesis_params: &GenesisParams,
    custom_genesis_state: Option<GenesisState>,
) -> Result<GenesisBatchParams, GenesisError> {
    let mut transaction = storage.start_transaction().await?;
    let verifier_config = L1VerifierConfig {
        snark_wrapper_vk_hash: genesis_params.config.snark_wrapper_vk_hash,
        fflonk_snark_wrapper_vk_hash: genesis_params.config.fflonk_snark_wrapper_vk_hash,
    };

    // if a custom genesis state was provided, read storage logs and factory dependencies from there
    let (storage_logs, factory_deps): (Vec<StorageLog>, HashMap<H256, Vec<u8>>) =
        match custom_genesis_state {
            Some(r) => (
                r.storage_logs
                    .into_iter()
                    .map(|x| StorageLog::from(&x))
                    .collect(),
                r.factory_deps
                    .into_iter()
                    .map(|f| (H256(f.bytecode_hash), f.bytecode))
                    .collect(),
            ),
            None => (
                get_storage_logs(&genesis_params.system_contracts),
                genesis_params
                    .system_contracts
                    .iter()
                    .map(|c| {
                        (
                            BytecodeHash::for_bytecode(&c.bytecode).value(),
                            c.bytecode.clone(),
                        )
                    })
                    .collect(),
            ),
        };

    // This action disregards how leaf indeces used to be ordered before, and it reorders them by
    // sorting by <address, key>, which is required for calculating genesis parameters.
    let deduped_log_queries = create_genesis_l1_batch_from_storage_logs_and_factory_deps(
        &mut transaction,
        genesis_params.protocol_version(),
        genesis_params.base_system_contracts(),
        &storage_logs,
        factory_deps,
        verifier_config,
    )
    .await?;
    tracing::info!("chain_schema_genesis is complete");

    let base_system_contract_hashes = BaseSystemContractsHashes {
        bootloader: genesis_params
            .config
            .bootloader_hash
            .ok_or(GenesisError::MalformedConfig("bootloader"))?,
        default_aa: genesis_params
            .config
            .default_aa_hash
            .ok_or(GenesisError::MalformedConfig("default_aa_hash"))?,
        evm_emulator: genesis_params.config.evm_emulator_hash,
    };

    let (genesis_batch_params, block_commitment) = make_genesis_batch_params(
        deduped_log_queries,
        base_system_contract_hashes,
        genesis_params.minor_protocol_version(),
    );

    save_genesis_l1_batch_metadata(
        &mut transaction,
        block_commitment.clone(),
        genesis_batch_params.root_hash,
        genesis_batch_params.rollup_last_leaf_index,
    )
    .await?;
    transaction.commit().await?;

    Ok(genesis_batch_params)
}

// Insert genesis batch into the database
pub async fn insert_genesis_batch(
    storage: &mut Connection<'_, Core>,
    genesis_params: &GenesisParams,
) -> Result<GenesisBatchParams, GenesisError> {
    insert_genesis_batch_with_custom_state(storage, genesis_params, None).await
}

pub async fn is_genesis_needed(storage: &mut Connection<'_, Core>) -> Result<bool, GenesisError> {
    Ok(storage.blocks_dal().is_genesis_needed().await?)
}

pub async fn validate_genesis_params(
    genesis_params: &GenesisParams,
    query_client: &dyn EthInterface,
    diamond_proxy_address: Address,
) -> anyhow::Result<()> {
    let hyperchain_abi = hyperchain_contract();
    let verifier_abi = verifier_contract();

    let packed_protocol_version: U256 = CallFunctionArgs::new("getProtocolVersion", ())
        .for_contract(diamond_proxy_address, &hyperchain_abi)
        .call(query_client)
        .await?;

    let protocol_version = ProtocolSemanticVersion::try_from_packed(packed_protocol_version)
        .map_err(|err| anyhow::format_err!("Failed to unpack semver: {err}"))?;

    if protocol_version != genesis_params.protocol_version() {
        return Err(anyhow::anyhow!(
            "Protocol version mismatch: {protocol_version} on contract, {} in config",
            genesis_params.protocol_version()
        ));
    }

    let verifier_address: Address = CallFunctionArgs::new("getVerifier", ())
        .for_contract(diamond_proxy_address, &hyperchain_abi)
        .call(query_client)
        .await?;

    let verification_key_hash: H256 = CallFunctionArgs::new("verificationKeyHash", ())
        .for_contract(verifier_address, &verifier_abi)
        .call(query_client)
        .await?;

    if verification_key_hash != genesis_params.config().snark_wrapper_vk_hash {
        return Err(anyhow::anyhow!(
            "Verification key hash mismatch: {verification_key_hash:?} on contract, {:?} in config",
            genesis_params.config().snark_wrapper_vk_hash
        ));
    }

    // We are getting function separately to get the second function with the same name, but
    // overriden one
    let function = verifier_abi
        .functions_by_name("verificationKeyHash")?
        .get(1);

    if let Some(function) = function {
        let fflonk_verification_key_hash: Option<H256> =
            CallFunctionArgs::new("verificationKeyHash", U256::from(0))
                .for_contract(verifier_address, &verifier_abi)
                .call_with_function(query_client, function.clone())
                .await
                .ok();
        tracing::info!(
            "FFlonk verification key hash in contract: {:?}",
            fflonk_verification_key_hash
        );
        tracing::info!(
            "FFlonk verification key hash in config: {:?}",
            genesis_params.config().fflonk_snark_wrapper_vk_hash
        );

        if fflonk_verification_key_hash.is_some()
            && fflonk_verification_key_hash != genesis_params.config().fflonk_snark_wrapper_vk_hash
        {
            return Err(anyhow::anyhow!(
            "FFlonk verification key hash mismatch: {fflonk_verification_key_hash:?} on contract, {:?} in config",
            genesis_params.config().fflonk_snark_wrapper_vk_hash
        ));
        }
    } else {
        tracing::warn!("FFlonk verification key hash is not present in the contract");
    }

    Ok(())
}

pub async fn ensure_genesis_state(
    storage: &mut Connection<'_, Core>,
    genesis_params: &GenesisParams,
    custom_genesis_state: Option<GenesisState>,
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
    } = insert_genesis_batch_with_custom_state(
        &mut transaction,
        genesis_params,
        custom_genesis_state,
    )
    .await?;

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

pub(crate) async fn create_genesis_l1_batch_from_storage_logs_and_factory_deps(
    storage: &mut Connection<'_, Core>,
    protocol_version: ProtocolSemanticVersion,
    base_system_contracts: &BaseSystemContracts,
    storage_logs: &[StorageLog],
    factory_deps: HashMap<H256, Vec<u8>>,
    l1_verifier_config: L1VerifierConfig,
) -> Result<Vec<LogQuery>, GenesisError> {
    let version = ProtocolVersion {
        version: protocol_version,
        timestamp: 0,
        l1_verifier_config,
        base_system_contracts_hashes: base_system_contracts.hashes(),
        tx: None,
    };

    let genesis_l1_batch_header = L1BatchHeader::new(
        L1BatchNumber(0),
        0,
        base_system_contracts.hashes(),
        protocol_version.minor,
    );
    let batch_fee_input = BatchFeeInput::pubdata_independent(0, 0, 0);

    let genesis_l2_block_header = L2BlockHeader {
        number: L2BlockNumber(0),
        timestamp: 0,
        hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Default::default(),
        base_fee_per_gas: 0,
        gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(protocol_version.minor.into()),
        batch_fee_input,
        base_system_contracts_hashes: base_system_contracts.hashes(),
        protocol_version: Some(protocol_version.minor),
        virtual_blocks: 0,
        gas_limit: 0,
        logs_bloom: Bloom::zero(),
        pubdata_params: Default::default(),
        rolling_txs_hash: Some(H256::zero()),
        // kl todo think through if adding proper genesis settlement layer here is needed.
        settlement_layer: SettlementLayer::L1(SLChainId(39)),
    };

    let mut transaction = storage.start_transaction().await?;

    transaction
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&version)
        .await?;
    transaction
        .blocks_dal()
        .insert_l1_batch(genesis_l1_batch_header.to_unsealed_header())
        .await?;
    transaction
        .blocks_dal()
        .mark_l1_batch_as_sealed(
            &genesis_l1_batch_header,
            &[],
            &[],
            &[],
            Default::default(),
            ZK_SYNC_BYTES_PER_BLOB as u64,
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

    insert_base_system_contracts_to_factory_deps(&mut transaction, base_system_contracts).await?;

    let mut genesis_transaction = transaction.start_transaction().await?;

    insert_storage_logs(&mut genesis_transaction, storage_logs).await?;
    let dedup_log_queries = get_deduped_log_queries(storage_logs);
    insert_deduplicated_writes_and_protective_reads(
        &mut genesis_transaction,
        dedup_log_queries.as_slice(),
    )
    .await?;
    insert_factory_deps(&mut genesis_transaction, factory_deps).await?;
    genesis_transaction.commit().await?;
    add_eth_token(&mut transaction).await?;

    transaction.commit().await?;
    Ok(dedup_log_queries)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_genesis_l1_batch(
    storage: &mut Connection<'_, Core>,
    protocol_version: ProtocolSemanticVersion,
    base_system_contracts: &BaseSystemContracts,
    system_contracts: &[DeployedContract],
    l1_verifier_config: L1VerifierConfig,
) -> Result<(), GenesisError> {
    let storage_logs = get_storage_logs(system_contracts);

    let factory_deps = system_contracts
        .iter()
        .map(|c| {
            (
                BytecodeHash::for_bytecode(&c.bytecode).value(),
                c.bytecode.clone(),
            )
        })
        .collect();

    create_genesis_l1_batch_from_storage_logs_and_factory_deps(
        storage,
        protocol_version,
        base_system_contracts,
        &storage_logs,
        factory_deps,
        l1_verifier_config,
    )
    .await?;
    Ok(())
}

// Save chain id transaction into the database
// We keep returning anyhow and will refactor it later
pub async fn save_set_chain_id_tx(
    storage: &mut Connection<'_, Core>,
    query_client: &dyn EthInterface,
    diamond_proxy_address: Address,
    event_expiration_blocks: u64,
) -> anyhow::Result<()> {
    let to = query_client.block_number().await?.as_u64();
    let from = to.saturating_sub(event_expiration_blocks);

    let filter = FilterBuilder::default()
        .address(vec![diamond_proxy_address])
        .topics(
            Some(vec![GENESIS_UPGRADE_EVENT.signature()]),
            Some(vec![diamond_proxy_address.into()]),
            None,
            None,
        )
        .from_block(from.into())
        .to_block(BlockNumber::Latest)
        .build();
    let mut logs = query_client.logs(&filter).await?;
    anyhow::ensure!(
        logs.len() == 1,
        "Expected a single set_chain_id event, got these {}: {:?}",
        logs.len(),
        logs
    );
    let (version_id, upgrade_tx) =
        decode_genesis_upgrade_event(logs.remove(0)).context("Chain id event is incorrect")?;

    tracing::info!("New version id {:?}", version_id);
    storage
        .protocol_versions_dal()
        .save_genesis_upgrade_with_tx(version_id, &upgrade_tx)
        .await?;
    Ok(())
}
