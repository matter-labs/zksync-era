//! This module aims to provide a genesis setup for the ZKsync Era network.
//! It initializes the Merkle tree with the basic setup (such as fields of special service accounts),
//! setups the required databases, and outputs the data required to initialize a smart contract.

use std::hash::Hash;
use std::{collections::HashMap, fmt::Formatter};
use std::str::FromStr;
use anyhow::Context as _;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::utils::Bytes32;
use zk_os_basic_system::system_implementation::flat_storage_model::{VersioningData, ACCOUNT_PROPERTIES_STORAGE_ADDRESS, DEFAULT_CODE_VERSION_BYTE};
use zksync_config::GenesisConfig;
use zksync_contracts::{
    hyperchain_contract, verifier_contract, BaseSystemContracts, BaseSystemContractsHashes,
    GENESIS_UPGRADE_EVENT,
};
use zksync_dal::{custom_genesis_export_dal::GenesisState, Connection, Core, CoreDal, DalError};
use zksync_eth_client::{CallFunctionArgs, EthInterface};
use zksync_merkle_tree::{domain::ZkSyncTree, TreeInstruction};
use zksync_multivm::utils::get_max_gas_per_pubdata_byte;
use zksync_multivm::zk_evm_latest::blake2::{Blake2s256, Digest};
use zksync_system_constants::PRIORITY_EXPIRATION;
use zksync_types::system_contracts::get_zk_os_system_smart_contracts;
use zksync_types::{address_to_h256, StorageLogKind};
use zksync_types::web3::keccak256;
use zksync_types::{block::{DeployedContract, L1BatchHeader, L2BlockHasher, L2BlockHeader}, bytecode::BytecodeHash, commitment::{CommitmentInput, L1BatchCommitment}, fee_model::BatchFeeInput, protocol_upgrade::decode_genesis_upgrade_event, protocol_version::{L1VerifierConfig, ProtocolSemanticVersion}, system_contracts::get_system_smart_contracts, u256_to_h256, web3::{BlockNumber, FilterBuilder}, zk_evm_types::LogQuery, AccountTreeId, Address, Bloom, L1BatchNumber, L1ChainId, L2BlockNumber, L2ChainId, ProtocolVersion, ProtocolVersionId, StorageKey, StorageLog, H256, U256, ethabi};
use zksync_types::block::L1BatchTreeData;
use zksync_types::ethabi::Token;
use zksync_zk_os_merkle_tree::TreeEntry;
use zksync_zkos_vm_runner::zkos_conversions::{b160_to_address, bytes32_to_h256, convert_boojum_account_properties};

use crate::utils::{
    add_eth_token, get_deduped_log_queries, get_storage_logs, insert_account_data_preimages, insert_base_system_contracts_to_factory_deps, insert_deduplicated_writes_and_protective_reads, insert_factory_deps, insert_storage_logs, process_genesis_batch_in_tree, save_genesis_l1_batch_metadata
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
        let system_contracts = get_zk_os_system_smart_contracts();
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
    let tree_entries = deduped_log_queries
        .into_iter()
        .filter(|log_query| log_query.rw_flag) // only writes
        .map(|log| {
            let storage_key =
                StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key));
            TreeEntry {
                key: storage_key.hashed_key(),
                value: u256_to_h256(log.written_value),
            }
        })
        .collect::<Vec<_>>();

    // Tree will insert guard leaves automatically, they don't need to be passed here.
    let metadata = process_genesis_batch_in_tree(&tree_entries);
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

/// Returns storage logs and factory deps for zk os contracts.
fn get_zk_os_genesis_logs(contracts: &[DeployedContract]) -> (Vec<StorageLog>, HashMap<H256, Vec<u8>>, HashMap<Address, zksync_types::boojum_os::AccountProperties>) {
    println!("{:#?}", contracts);

    let mut logs = vec![];
    let mut factory_deps =HashMap::new();
    let mut account_data_preimages = HashMap::new();
    for contract in contracts {
        let bytecode_hash = {
            let digest = Blake2s256::digest(&contract.bytecode);
            let mut digest_array = [0u8; 32];
            digest_array.copy_from_slice(&digest.as_slice());
            Bytes32::from_array(digest_array)
        };
        let observable_bytecode_hash = {
            let digest = keccak256(&contract.bytecode);
            Bytes32::from_array(digest)
        };

        let mut versioning_data = VersioningData::empty_non_deployed();
        versioning_data.set_as_deployed();
        versioning_data.set_code_version(DEFAULT_CODE_VERSION_BYTE);
        versioning_data.set_ee_version(ExecutionEnvironmentType::EVM as u8);
            
        // TODO: why do we have a copy of the same struct in types?
        let properties = zk_os_basic_system::system_implementation::flat_storage_model::AccountProperties {
            versioning_data: versioning_data,
            // When contracts are deployed, they have a nonce of 1
            nonce: 1,
            observable_bytecode_hash: observable_bytecode_hash,
            bytecode_hash: bytecode_hash,
            bytecode_len: contract.bytecode.len() as u32,
            artifacts_len: 0,
            observable_bytecode_len: contract.bytecode.len() as u32,
            balance: Default::default(),
        };
        let properties_hash = H256(properties.compute_hash().as_u8_array());
        let storage_log = StorageLog {
            kind: StorageLogKind::InitialWrite,
            key: StorageKey::new(AccountTreeId::new(b160_to_address(ACCOUNT_PROPERTIES_STORAGE_ADDRESS)), address_to_h256(contract.account_id.address())),
            value: properties_hash,
        };
        factory_deps.insert(bytes32_to_h256(bytecode_hash), contract.bytecode.clone());
        account_data_preimages.insert(*contract.account_id.address(), convert_boojum_account_properties(properties));
        logs.push(storage_log);
    }
    (logs, factory_deps, account_data_preimages)
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
    println!("hi!2");

    // if a custom genesis state was provided, read storage logs and factory dependencies from there
    let (storage_logs, factory_deps, account_data_preimages): (Vec<StorageLog>, HashMap<H256, Vec<u8>>, HashMap<Address, zksync_types::boojum_os::AccountProperties>) =
        match custom_genesis_state {
            Some(r) => (
                panic!("unsupported")
                // r.storage_logs
                //     .into_iter()
                //     .map(|x| StorageLog::from(&x))
                //     .collect(),
                // r.factory_deps
                //     .into_iter()
                //     .map(|f| (H256(f.bytecode_hash), f.bytecode))
                //     .collect(),
            ),
            None => get_zk_os_genesis_logs(&genesis_params.system_contracts()),
        };
    println!("hi!");

    // This action disregards how leaf indeces used to be ordered before, and it reorders them by
    // sorting by <address, key>, which is required for calculating genesis parameters.
    let deduped_log_queries = create_genesis_l1_batch_from_storage_logs_and_factory_deps(
        &mut transaction,
        genesis_params.protocol_version(),
        genesis_params.base_system_contracts(),
        &storage_logs,
        factory_deps,
        verifier_config,
        account_data_preimages
    )
    .await?;
    println!("hi!3");

    tracing::info!("chain_schema_genesis is complete. Deduped log queries: {:?}", deduped_log_queries);

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

    let tree_entries = deduped_log_queries
        .into_iter()
        .filter(|log_query| log_query.rw_flag) // only writes
        .map(|log| {
            let storage_key =
                StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key));
            TreeEntry {
                key: storage_key.hashed_key(),
                value: u256_to_h256(log.written_value),
            }
        })
        .collect::<Vec<_>>();

    let metadata = process_genesis_batch_in_tree(&tree_entries);

    // this commitment matches computation in
    // core/lib/l1_contract_interface/src/i_executor/structures/stored_batch_info.rs
    //
    // code will be reused after commitment refactor

    let commitment: H256 = H256::from_str("0x753b52ab98b0062963a4b2ea1c061c4ab522f53f50b8fefe0a52760cbcc9e183").unwrap();

    // note: for zkos `genesis_root` is blake(root, slot_index)
    // but the same field is reused over smart contract side - so it has differnt meanings for pre-zkos / zkos.
    // so we use the same field here as well

    let mut hasher = Blake2s256::new();
    hasher.update(metadata.root_hash.as_bytes()); // as_u8_ref in ruint
    hasher.update(metadata.leaf_count.to_be_bytes()); // as_be_bytes in ruint

    // return the final hash
    let zkos_root_batch_hash= H256::from_slice(&hasher.finalize());

    let genesis_batch_params = GenesisBatchParams {
        root_hash: zkos_root_batch_hash,
        rollup_last_leaf_index: 0, // hardcoded in zkos - should be set to `0` for all batches
        commitment,
    };

    let tree_data = L1BatchTreeData {
        hash: metadata.root_hash,
        rollup_last_leaf_index: metadata.leaf_count - 1,  // here we dont set zero - as this field is used fopr state_commitment
    };

    transaction
        .blocks_dal()
        .save_l1_batch_tree_data(L1BatchNumber(0), &tree_data)
        .await?;


    transaction
        .blocks_dal()
        // set to zero in DEFAULT_L2_LOGS_TREE_ROOT_HASH in
        // /Users/romanbrodetskiy/src/zksync-era/contracts/l1-contracts/contracts/common/Config.sol
        .insert_l2_l1_message_root(L1BatchNumber(0), H256::zero())
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
    account_data_preimages: HashMap<Address, zksync_types::boojum_os::AccountProperties>,
) -> Result<Vec<LogQuery>, GenesisError> {
    let version = ProtocolVersion {
        version: protocol_version,
        timestamp: 0,
        l1_verifier_config,
        base_system_contracts_hashes: base_system_contracts.hashes(),
        tx: None,
    };
    println!("hi!4");

    let genesis_l1_batch_header = L1BatchHeader::new(
        L1BatchNumber(0),
        0,
        base_system_contracts.hashes(),
        protocol_version.minor,
    );
    println!("hi!5");

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
    };
    println!("hi!6");

    let mut transaction = storage.start_transaction().await?;
    println!("hi!7");

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
        .mark_l1_batch_as_sealed(&genesis_l1_batch_header, &[], &[], &[], Default::default())
        .await?;
    transaction
        .blocks_dal()
        .insert_l2_block(&genesis_l2_block_header)
        .await?;
    transaction
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(0))
        .await?;
    println!("hi!8");

    insert_base_system_contracts_to_factory_deps(&mut transaction, base_system_contracts).await?;
    println!("hi!9");

    let mut genesis_transaction = transaction.start_transaction().await?;

    insert_storage_logs(&mut genesis_transaction, storage_logs).await?;
    println!("hi!10");

    let dedup_log_queries = get_deduped_log_queries(storage_logs);
    println!("hi!15");

    insert_deduplicated_writes_and_protective_reads(
        &mut genesis_transaction,
        dedup_log_queries.as_slice(),
    )
    .await?;
    println!("hi!11");

    insert_factory_deps(&mut genesis_transaction, factory_deps).await?;

    insert_account_data_preimages(&mut genesis_transaction, account_data_preimages).await?;
    println!("hi!12");
    genesis_transaction.commit().await?;
    println!("hi!13");
    add_eth_token(&mut transaction).await?;
    println!("hi!14");
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

    let (storage_logs, factory_deps, account_data_preimages) = get_zk_os_genesis_logs(system_contracts);

    create_genesis_l1_batch_from_storage_logs_and_factory_deps(
        storage,
        protocol_version,
        base_system_contracts,
        &storage_logs,
        factory_deps,
        l1_verifier_config,
        account_data_preimages,
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
) -> anyhow::Result<()> {
    let to = query_client.block_number().await?.as_u64();
    let from = to.saturating_sub(PRIORITY_EXPIRATION);

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
