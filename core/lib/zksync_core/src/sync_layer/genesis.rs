use anyhow::Context as _;

use zksync_dal::StorageProcessor;
use zksync_types::{
    block::DeployedContract, protocol_version::L1VerifierConfig,
    system_contracts::get_system_smart_contracts, AccountTreeId, Address, L1BatchNumber, L2ChainId,
    H256,
};

use super::client::MainNodeClient;
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
        let genesis_params = create_genesis_params(client).await?;
        ensure_genesis_state(&mut transaction, zksync_chain_id, &genesis_params)
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

async fn create_genesis_params(client: &dyn MainNodeClient) -> anyhow::Result<GenesisParams> {
    let genesis_miniblock = client
        .fetch_l2_block(zksync_types::MiniblockNumber(0), false)
        .await?
        .context("No genesis block on the main node")?;
    let first_validator = genesis_miniblock.operator_address;
    let base_system_contracts_hashes = genesis_miniblock.base_system_contracts_hashes;
    let protocol_version = genesis_miniblock.protocol_version;

    // Load the list of addresses that are known to contain system contracts at any point in time.
    // Not every of these addresses is guaranteed to be present in the genesis state, but we'll iterate through
    // them and try to fetch the contract bytecode for each of them.
    let system_contract_addresses: Vec<_> = get_system_smart_contracts()
        .into_iter()
        .map(|contract| *contract.account_id.address())
        .collect();

    // These have to be *initial* base contract hashes of main node
    // (those that were used during genesis), not necessarily the current ones.
    let base_system_contracts = client
        .fetch_base_system_contracts(base_system_contracts_hashes)
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
    Ok(GenesisParams {
        protocol_version,
        base_system_contracts,
        system_contracts,
        first_validator,
        first_l1_verifier_config,
        first_verifier_address,
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
