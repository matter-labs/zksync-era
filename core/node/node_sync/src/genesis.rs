use anyhow::Context as _;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::{custom_genesis_export_dal::GenesisState, Connection, Core, CoreDal};
use zksync_node_genesis::{ensure_genesis_state, GenesisParams};
use zksync_types::{
    block::DeployedContract, system_contracts::get_system_smart_contracts, AccountTreeId, L2ChainId,
};

use super::client::MainNodeClient;

pub async fn is_genesis_needed(storage: &mut Connection<'_, Core>) -> anyhow::Result<bool> {
    Ok(storage.blocks_dal().is_genesis_needed().await?)
}

pub async fn perform_genesis_if_needed(
    storage: &mut Connection<'_, Core>,
    zksync_chain_id: L2ChainId,
    client: &dyn MainNodeClient,
    custom_genesis_state: Option<GenesisState>,
) -> anyhow::Result<()> {
    let mut transaction = storage.start_transaction().await?;
    // We want to check whether the genesis is needed before we create genesis params to not
    // make the node startup slower.
    if transaction.blocks_dal().is_genesis_needed().await? {
        let genesis_params = create_genesis_params(client, zksync_chain_id).await?;
        ensure_genesis_state(&mut transaction, &genesis_params, custom_genesis_state)
            .await
            .context("ensure_genesis_state")?;
    }
    transaction.commit().await?;
    Ok(())
}

async fn create_genesis_params(
    client: &dyn MainNodeClient,
    zksync_chain_id: L2ChainId,
) -> anyhow::Result<GenesisParams> {
    let config = client.fetch_genesis_config().await?;
    let base_system_contracts_hashes = BaseSystemContractsHashes {
        bootloader: config.bootloader_hash.context("Genesis is not finished")?,
        default_aa: config.default_aa_hash.context("Genesis is not finished")?,
        evm_emulator: config.evm_emulator_hash,
    };

    if zksync_chain_id != config.l2_chain_id {
        anyhow::bail!("L2 chain id from server and locally doesn't match");
    }

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
            // It's OK for some contracts to be absent.
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
    let evm_emulator = if let Some(hash) = contract_hashes.evm_emulator {
        let bytes = client
            .fetch_system_contract_by_hash(hash)
            .await?
            .context("EVM emulator bytecode is missing on main node")?;
        Some(SystemContractCode { code: bytes, hash })
    } else {
        None
    };
    Ok(BaseSystemContracts {
        bootloader: SystemContractCode {
            code: bootloader_bytecode,
            hash: contract_hashes.bootloader,
        },
        default_aa: SystemContractCode {
            code: default_aa_bytecode,
            hash: contract_hashes.default_aa,
        },
        evm_emulator,
    })
}
