use crate::genesis::{ensure_genesis_state, GenesisParams};

use anyhow::Context;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::StorageProcessor;
use zksync_types::{
    api, block::DeployedContract, get_code_key, protocol_version::L1VerifierConfig,
    system_contracts::get_system_smart_contracts, AccountTreeId, Address, L1BatchNumber, L2ChainId,
    MiniblockNumber, ACCOUNT_CODE_STORAGE_ADDRESS, H256, U64,
};
use zksync_utils::h256_to_u256;
use zksync_web3_decl::{
    jsonrpsee::{core::error::Error, http_client::HttpClientBuilder},
    namespaces::{EnNamespaceClient, EthNamespaceClient, ZksNamespaceClient},
};

pub async fn perform_genesis_if_needed(
    storage: &mut StorageProcessor<'_>,
    zksync_chain_id: L2ChainId,
    main_node_url: String,
) -> anyhow::Result<()> {
    let mut transaction = storage.start_transaction().await;
    // We want to check whether the genesis is needed before we create genesis params to not
    // make the node startup slower.
    let genesis_block_hash = if transaction.blocks_dal().is_genesis_needed().await {
        let genesis_params = create_genesis_params(&main_node_url).await?;
        let genesis_block_hash =
            ensure_genesis_state(&mut transaction, zksync_chain_id, &genesis_params).await;
        genesis_block_hash
    } else {
        transaction
            .blocks_dal()
            .get_l1_batch_state_root(L1BatchNumber(0))
            .await
            .expect("genesis block hash is empty")
    };

    validate_genesis_state(&main_node_url, genesis_block_hash).await;
    transaction.commit().await;

    Ok(())
}

async fn create_genesis_params(main_node_url: &str) -> anyhow::Result<GenesisParams> {
    let base_system_contracts_hashes = fetch_genesis_system_contracts(main_node_url)
        .await
        .context("Unable to fetch genesis system contracts hashes")?;

    // Load the list of addresses that are known to contain system contracts at any point in time.
    // Not every of these addresses is guaranteed to be present in the genesis state, but we'll iterate through
    // them and try to fetch the contract bytecode for each of them.
    let system_contract_addresses: Vec<_> = get_system_smart_contracts()
        .into_iter()
        .map(|contract| *contract.account_id.address())
        .collect();

    // These have to be *initial* base contract hashes of main node
    // (those that were used during genesis), not necessarily the current ones.
    let base_system_contracts =
        fetch_base_system_contracts(main_node_url, base_system_contracts_hashes)
            .await
            .context("Failed to fetch base system contracts from main node")?;

    let client = HttpClientBuilder::default().build(main_node_url).unwrap();
    let first_validator = client
        .get_block_details(MiniblockNumber(0))
        .await
        .context("Unable to fetch genesis block details")?
        .context("Failed to fetch genesis miniblock")?
        .operator_address;

    // In EN, we don't know what were the system contracts at the genesis state.
    // We know the list of addresses where these contracts *may have been* deployed.
    // So, to collect the list of system contracts, we compute the corresponding storage slots and request
    // the state at genesis block to fetch the hash of the corresponding contract.
    // Then, we can fetch the factory dependency bytecode to fully recover the contract.
    let mut system_contracts: Vec<DeployedContract> =
        Vec::with_capacity(system_contract_addresses.len());
    const GENESIS_BLOCK: api::BlockIdVariant =
        api::BlockIdVariant::BlockNumber(api::BlockNumber::Number(U64([0])));
    for system_contract_address in system_contract_addresses {
        let code_key = get_code_key(&system_contract_address);
        let code_hash = client
            .get_storage_at(
                ACCOUNT_CODE_STORAGE_ADDRESS,
                h256_to_u256(*code_key.key()),
                Some(GENESIS_BLOCK),
            )
            .await
            .context("Unable to query storage at genesis state")?;
        let Some(bytecode) = client
            .get_bytecode_by_hash(code_hash)
            .await
            .context("Unable to query system contract bytecode")?
        else {
            // It's OK for some of contracts to be absent.
            // If this is a bug, the genesis root hash won't match.
            vlog::debug!("System contract with address {system_contract_address:?} is absent at genesis state");
            continue;
        };
        let contract = DeployedContract::new(AccountTreeId::new(system_contract_address), bytecode);
        system_contracts.push(contract);
    }
    assert!(
        !system_contracts.is_empty(),
        "No system contracts were fetched: this is a bug"
    );

    // Use default L1 verifier config and verifier address for genesis as they are not used by EN.
    let first_l1_verifier_config = L1VerifierConfig::default();
    let first_verifier_address = Address::default();
    Ok(GenesisParams {
        base_system_contracts,
        system_contracts,
        first_validator,
        first_l1_verifier_config,
        first_verifier_address,
    })
}

// When running an external node, we want to make sure we have the same
// genesis root hash as the main node.
async fn validate_genesis_state(main_node_url: &str, root_hash: H256) {
    let client = HttpClientBuilder::default().build(main_node_url).unwrap();
    let genesis_block = client
        .get_l1_batch_details(L1BatchNumber(0))
        .await
        .expect("couldn't get genesis block from the main node")
        .expect("main node did not return a genesis block");

    let genesis_block_hash = genesis_block
        .base
        .root_hash
        .expect("empty genesis block hash");

    if genesis_block_hash != root_hash {
        panic!(
            "Genesis block root hash mismatch with main node: expected {}, got {}",
            root_hash, genesis_block_hash
        );
    }
}

async fn fetch_genesis_system_contracts(
    main_node_url: &str,
) -> Result<BaseSystemContractsHashes, Error> {
    let client = HttpClientBuilder::default().build(main_node_url).unwrap();
    let hashes = client
        .sync_l2_block(zksync_types::MiniblockNumber(0), false)
        .await?
        .expect("No genesis block on the main node")
        .base_system_contracts_hashes;
    Ok(hashes)
}

pub async fn fetch_system_contract_by_hash(
    main_node_url: &str,
    hash: H256,
) -> Result<SystemContractCode, Error> {
    let client = HttpClientBuilder::default().build(main_node_url).unwrap();
    let bytecode = client.get_bytecode_by_hash(hash).await?.unwrap_or_else(|| {
        panic!(
            "Base system contract bytecode is absent on the main node. Dependency hash: {:?}",
            hash
        )
    });
    assert_eq!(
        hash,
        zksync_utils::bytecode::hash_bytecode(&bytecode),
        "Got invalid base system contract bytecode from main node"
    );
    Ok(SystemContractCode {
        code: zksync_utils::bytes_to_be_words(bytecode),
        hash,
    })
}

pub async fn fetch_base_system_contracts(
    main_node_url: &str,
    hashes: BaseSystemContractsHashes,
) -> Result<BaseSystemContracts, Error> {
    Ok(BaseSystemContracts {
        bootloader: fetch_system_contract_by_hash(main_node_url, hashes.bootloader).await?,
        default_aa: fetch_system_contract_by_hash(main_node_url, hashes.default_aa).await?,
    })
}
