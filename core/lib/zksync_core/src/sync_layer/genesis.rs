use anyhow::Context as _;
use zksync_config::GenesisConfig;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::StorageProcessor;
use zksync_types::{
    block::DeployedContract, protocol_version::L1VerifierConfig,
    system_contracts::get_system_smart_contracts, AccountTreeId, Address, L1BatchNumber, L2ChainId,
    H256,
};

use super::client::{GenesisContracts, MainNodeClient};
use crate::genesis::{ensure_genesis_state, GenesisParams};

pub async fn perform_genesis_if_needed(
    storage: &mut StorageProcessor<'_>,
    zksync_chain_id: L2ChainId,
    client: &dyn MainNodeClient,
) -> anyhow::Result<()> {
    let mut transaction = storage.start_transaction().await?;
    // We want to check whether the genesis is needed before we create genesis params to not
    // make the node startup slower.
    let genesis_block_hash = if transaction.blocks_dal().is_genesis_needed().await? {
        let genesis_params = create_genesis_params(client, zksync_chain_id).await?;
        ensure_genesis_state(&mut transaction, &genesis_params)
            .await
            .context("ensure_genesis_state")?
    } else {
        transaction
            .blocks_dal()
            .get_l1_batch_state_root(L1BatchNumber(0))
            .await?
            .context("genesis block hash is empty")?
    };

    validate_genesis_state(client, genesis_block_hash).await?;
    transaction.commit().await?;
    Ok(())
}

async fn create_genesis_params(
    client: &dyn MainNodeClient,
    zksync_chain_id: L2ChainId,
) -> anyhow::Result<GenesisParams> {
    let genesis_miniblock = client
        .fetch_l2_block(zksync_types::MiniblockNumber(0), false)
        .await?
        .context("No genesis block on the main node")?;
    let first_validator = genesis_miniblock.operator_address;
    let base_system_contracts_hashes = genesis_miniblock.base_system_contracts_hashes;
    let protocol_version = genesis_miniblock.protocol_version;
    let genesis_batch = client.fetch_genesis_l1_batch().await?;
    let l1_chain_id = client.fetch_l1_chain_id().await?;
    let GenesisContracts {
        diamond_proxy,
        bridges,
    } = client.fetch_genesis_contracts().await?;

    // Load the list of addresses that are known to contain system contracts at any point in time.
    // Not every of these addresses is guaranteed to be present in the genesis state, but we'll iterate through
    // them and try to fetch the contract bytecode for each of them.
    let system_contract_addresses: Vec<_> = get_system_smart_contracts()
        .into_iter()
        .map(|contract| *contract.account_id.address())
        .collect();

    // These have to be *initial* base contract hashes of main node
    // (those that were used during genesis), not necessarily the current ones.
    let base_system_contracts = fetch_base_system_contracts(client, base_system_contracts_hashes)
        .await
        .context("Failed to fetch base system contracts from main node")?;

    // In EN, we don't know what were the system contracts at the genesis state.
    // We know the list of addresses where these contracts *may have been* deployed.
    // So, to collect the list of system contracts, we compute the corresponding storage slots and request
    // the state at genesis block to fetch the hash of the corresponding contract.
    // Then, we can fetch the factory dependency bytecode to fully recover the contract.
    let mut system_contracts: Vec<DeployedContract> =
        Vec::with_capacity(system_contract_addresses.len());

    for system_contract_address in system_contract_addresses {
        let Some(bytecode) = client
            .fetch_genesis_contract_bytecode(system_contract_address)
            .await?
        else {
            // It's OK for some of contracts to be absent.
            // If this is a bug, the genesis root hash won't match.
            tracing::debug!("System contract with address {system_contract_address:?} is absent at genesis state");
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
    let config = GenesisConfig {
        protocol_version: protocol_version as u16,
        genesis_root_hash: genesis_batch
            .base
            .root_hash
            .context("Genesis can't be pending")?,
        rollup_last_leaf_index: genesis_batch
            .rollup_last_leaf_index
            .context("Genesis can't be pending")?,
        genesis_commitment: genesis_batch
            .commitment
            .context("Genesis can't be pending")?,
        bootloader_hash: base_system_contracts_hashes.bootloader,
        default_aa_hash: base_system_contracts_hashes.default_aa,
        verifier_address: first_verifier_address,
        fee_account: first_validator,
        diamond_proxy,
        erc20_bridge: bridges.l1_erc20_default_bridge,
        state_transition_proxy_addr: None,
        l1_chain_id,
        l2_chain_id: zksync_chain_id,
        recursion_node_level_vk_hash: first_l1_verifier_config.params.recursion_node_level_vk_hash,
        recursion_leaf_level_vk_hash: first_l1_verifier_config.params.recursion_leaf_level_vk_hash,
        recursion_circuits_set_vks_hash: first_l1_verifier_config
            .params
            .recursion_circuits_set_vks_hash,
        recursion_scheduler_level_vk_hash: first_l1_verifier_config
            .recursion_scheduler_level_vk_hash,
    };
    Ok(GenesisParams::from_genesis_config(
        config,
        base_system_contracts,
        system_contracts,
    )?)
}

async fn fetch_base_system_contracts(
    client: &dyn MainNodeClient,
    contract_hashes: BaseSystemContractsHashes,
) -> anyhow::Result<BaseSystemContracts> {
    let bootloader_bytecode = client
        .fetch_system_contract_by_hash(contract_hashes.bootloader)
        .await?
        .context("bootloader bytecode is missing on main node")?;
    let default_aa_bytecode = client
        .fetch_system_contract_by_hash(contract_hashes.default_aa)
        .await?
        .context("default AA bytecode is missing on main node")?;
    Ok(BaseSystemContracts {
        bootloader: SystemContractCode {
            code: zksync_utils::bytes_to_be_words(bootloader_bytecode),
            hash: contract_hashes.bootloader,
        },
        default_aa: SystemContractCode {
            code: zksync_utils::bytes_to_be_words(default_aa_bytecode),
            hash: contract_hashes.default_aa,
        },
    })
}

// When running an external node, we want to make sure we have the same
// genesis root hash as the main node.
async fn validate_genesis_state(
    client: &dyn MainNodeClient,
    root_hash: H256,
) -> anyhow::Result<()> {
    let genesis_l1_batch_hash = client.fetch_genesis_l1_batch_hash().await?;
    anyhow::ensure!(
        genesis_l1_batch_hash == root_hash,
        "Genesis L1 batch root hash mismatch with main node: expected {root_hash}, got {genesis_l1_batch_hash}"
    );
    Ok(())
}
