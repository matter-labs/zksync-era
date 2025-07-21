use anyhow::Context;
use ethers::{abi::parse_abi, contract::BaseContract, utils::hex};
use lazy_static::lazy_static;
use serde::Deserialize;
use xshell::{cmd, Shell};
use zkstack_cli_common::{forge::Forge, git, logger, spinner::Spinner};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::input::GenesisInput,
        script_params::{
            ForgeScriptParams, FINALIZE_UPGRADE_SCRIPT_PARAMS, V29_UPGRADE_ECOSYSTEM_PARAMS,
            ZK_OS_V28_1_UPGRADE_ECOSYSTEM_PARAMS,
        },
        upgrade_ecosystem::{input::EcosystemUpgradeInput, output::EcosystemUpgradeOutput},
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath},
    ContractsConfig, EcosystemConfig, GenesisConfig, GENESIS_FILE,
};
use zkstack_cli_types::ProverMode;
use zksync_basic_types::H160;
use zksync_types::{L2_NATIVE_TOKEN_VAULT_ADDRESS, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS, U256};

use crate::{
    admin_functions::governance_execute_calls,
    commands::dev::commands::upgrades::{
        args::ecosystem::{EcosystemUpgradeArgs, EcosystemUpgradeArgsFinal, EcosystemUpgradeStage},
        types::UpgradeVersions,
    },
    messages::MSG_INTALLING_DEPS_SPINNER,
    utils::forge::{fill_forge_private_key, WalletOwner},
};

pub async fn run(
    shell: &Shell,
    args: EcosystemUpgradeArgs,
    run_upgrade: bool,
) -> anyhow::Result<()> {
    println!("Running ecosystem gateway upgrade args");

    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let upgrade_version = args.upgrade_version;

    let mut final_ecosystem_args = args.fill_values_with_prompt(run_upgrade);

    match final_ecosystem_args.ecosystem_upgrade_stage {
        EcosystemUpgradeStage::NoGovernancePrepare => {
            no_governance_prepare(
                &mut final_ecosystem_args,
                shell,
                &ecosystem_config,
                &upgrade_version,
            )
            .await?;
        }
        EcosystemUpgradeStage::GovernanceStage0 => {
            governance_stage_0(
                &mut final_ecosystem_args,
                shell,
                &ecosystem_config,
                &upgrade_version,
            )
            .await?;
        }
        EcosystemUpgradeStage::GovernanceStage1 => {
            governance_stage_1(
                &mut final_ecosystem_args,
                shell,
                &ecosystem_config,
                &upgrade_version,
            )
            .await?;
        }
        EcosystemUpgradeStage::GovernanceStage2 => {
            governance_stage_2(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
        EcosystemUpgradeStage::NoGovernanceStage2 => {
            no_governance_stage_2(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct BroadcastFile {
    pub transactions: Vec<BroadcastFileTransactions>,
}
#[derive(Debug, Deserialize)]
struct BroadcastFileTransactions {
    pub hash: String,
}

async fn no_governance_prepare(
    init_args: &mut EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    upgrade_version: &UpgradeVersions,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let forge_args = init_args.forge_args.clone();
    let l1_rpc_url = if let Some(url) = init_args.l1_rpc_url.clone() {
        url
    } else {
        ecosystem_config
            .load_current_chain()?
            .get_secrets_config()
            .await?
            .l1_rpc_url()?
    };
    dbg!(&l1_rpc_url);

    let genesis_config_path = ecosystem_config
        .get_default_configs_path()
        .join(GENESIS_FILE);
    let default_genesis_config = GenesisConfig::read(shell, genesis_config_path).await?;
    let default_genesis_input = GenesisInput::new(&default_genesis_config)?;
    let current_contracts_config = ecosystem_config.get_contracts_config()?;
    let bridgehub_proxy_address = current_contracts_config
        .ecosystem_contracts
        .bridgehub_proxy_addr;

    let bridgehub_proxy_address_str = format!("{:#x}", bridgehub_proxy_address);

    logger::info(format!(
        "Executing: cast call {} \"messageRoot()(address)\" to get the current messageRoot address from BridgeHub.",
        bridgehub_proxy_address_str
    ));

    // Execute the cast call command.
    // The command is: cast call <BRIDGEHUB_ADDRESS> "messageRoot()(address)"
    // This retrieves the address of the messageRoot contract associated with the BridgeHub.
    let cast_output_stdout = cmd!(
        shell,
        "cast call {bridgehub_proxy_address_str} messageRoot()(address) -r {l1_rpc_url}"
    )
    .read()
    .context("Failed to execute 'cast call' to retrieve messageRoot address from BridgeHub.")?;

    // The output from `cast call` is typically the address followed by a newline.
    // Trim whitespace and store it.
    let message_root_address_from_cast = cast_output_stdout.trim().to_string();

    if message_root_address_from_cast.is_empty()
        || message_root_address_from_cast == "0x0000000000000000000000000000000000000000"
    {
        anyhow::bail!(
            "Retrieved messageRoot address from BridgeHub is empty or zero: '{}'. This indicates an issue.",
            message_root_address_from_cast
        );
    }

    logger::info(format!(
        "Successfully retrieved messageRoot address from BridgeHub: {}",
        message_root_address_from_cast
    ));

    let initial_deployment_config = ecosystem_config.get_initial_deployment_config()?;

    let ecosystem_upgrade_config_path = get_ecosystem_upgrade_params(&upgrade_version)
        .input(&ecosystem_config.path_to_l1_foundry());

    let mut new_genesis = default_genesis_input;
    let mut new_version = new_genesis.protocol_version;
    new_version.patch += 1;
    new_genesis.protocol_version = new_version;

    let ecosystem_upgrade = EcosystemUpgradeInput::new(
        &new_genesis,
        &current_contracts_config,
        &initial_deployment_config,
        ecosystem_config.era_chain_id,
        ecosystem_config
            .get_contracts_config()?
            .l1
            .diamond_proxy_addr,
        ecosystem_config.prover_version == ProverMode::NoProofs,
    );

    logger::info(format!("ecosystem_upgrade: {:?}", ecosystem_upgrade));
    logger::info(format!(
        "ecosystem_upgrade_config_path: {:?}",
        ecosystem_upgrade_config_path
    ));
    ecosystem_upgrade.save(shell, ecosystem_upgrade_config_path.clone())?;
    let mut forge = Forge::new(&ecosystem_config.path_to_l1_foundry())
        .script(
            &get_ecosystem_upgrade_params(&upgrade_version).script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_slow()
        .with_gas_limit(1_000_000_000_000)
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.deployer.as_ref(),
        WalletOwner::Deployer,
    )?;

    logger::info(format!("Preparing the ecosystem for the upgrade!"));

    forge.run(shell)?;

    logger::info("done!");

    let l1_chain_id = ecosystem_config.l1_network.chain_id();

    let broadcast_file: BroadcastFile = {
        let file_content =
            std::fs::read_to_string(ecosystem_config.path_to_l1_foundry().join(format!(
                "broadcast/EcosystemUpgrade_v29.s.sol/{}/run-latest.json",
                l1_chain_id
            )))
            .context("Failed to read broadcast file")?;
        serde_json::from_str(&file_content).context("Failed to parse broadcast file")?
    };

    let mut output = EcosystemUpgradeOutput::read(
        shell,
        get_ecosystem_upgrade_params(&upgrade_version)
            .output(&ecosystem_config.path_to_l1_foundry()),
    )?;

    // Add all the transaction hashes.
    for tx in broadcast_file.transactions {
        output.transactions.push(tx.hash);
    }

    output.save_with_base_path(shell, &ecosystem_config.config)?;

    Ok(())
}

async fn governance_stage_0(
    init_args: &mut EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    upgrade_version: &UpgradeVersions,
) -> anyhow::Result<()> {
    let spinner = Spinner::new("Executing governance stage 0!");

    let previous_output = EcosystemUpgradeOutput::read(
        shell,
        get_ecosystem_upgrade_params(&upgrade_version)
            .output(&ecosystem_config.path_to_l1_foundry()),
    )?;
    previous_output.save_with_base_path(shell, &ecosystem_config.config)?;
    let l1_rpc_url = if let Some(url) = init_args.l1_rpc_url.clone() {
        url
    } else {
        ecosystem_config
            .load_current_chain()?
            .get_secrets_config()
            .await?
            .l1_rpc_url()?
    };

    // These are ABI-encoded
    let stage0_calls = previous_output.governance_calls.stage0_calls;

    governance_execute_calls(
        shell,
        ecosystem_config,
        &ecosystem_config.get_wallets()?.governor,
        stage0_calls.0,
        &init_args.forge_args.clone(),
        l1_rpc_url,
    )
    .await?;
    spinner.finish();

    Ok(())
}

// Governance has approved the proposal, now it will insert the new protocol version into our STM (CTM)
async fn governance_stage_1(
    init_args: &mut EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    upgrade_version: &UpgradeVersions,
) -> anyhow::Result<()> {
    println!("Executing governance stage 1!");

    let previous_output = EcosystemUpgradeOutput::read(
        shell,
        get_ecosystem_upgrade_params(&upgrade_version)
            .output(&ecosystem_config.path_to_l1_foundry()),
    )?;
    previous_output.save_with_base_path(shell, &ecosystem_config.config)?;

    // These are ABI-encoded
    let stage1_calls = previous_output.governance_calls.stage1_calls;
    let l1_rpc_url = if let Some(url) = init_args.l1_rpc_url.clone() {
        url
    } else {
        ecosystem_config
            .load_current_chain()?
            .get_secrets_config()
            .await?
            .l1_rpc_url()?
    };

    governance_execute_calls(
        shell,
        ecosystem_config,
        &ecosystem_config.get_wallets()?.governor,
        stage1_calls.0,
        &init_args.forge_args.clone(),
        l1_rpc_url.clone(),
    )
    .await?;

    let gateway_ecosystem_preparation_output =
        EcosystemUpgradeOutput::read_with_base_path(shell, &ecosystem_config.config)?;

    let mut contracts_config = ecosystem_config.get_contracts_config()?;

    update_contracts_config_from_output(
        &mut contracts_config,
        &gateway_ecosystem_preparation_output,
    );

    // This value is meaningless for the ecosystem, but we'll populate it for consistency
    contracts_config.l2.da_validator_addr = Some(H160::zero());
    contracts_config.l2.l2_native_token_vault_proxy_addr = Some(L2_NATIVE_TOKEN_VAULT_ADDRESS);
    contracts_config.l2.legacy_shared_bridge_addr = contracts_config.bridges.shared.l2_address;

    contracts_config.save_with_base_path(shell, &ecosystem_config.config)?;

    Ok(())
}

fn update_contracts_config_from_output(
    contracts_config: &mut ContractsConfig,
    output: &EcosystemUpgradeOutput,
) {
    contracts_config
        .ecosystem_contracts
        .stm_deployment_tracker_proxy_addr = Some(
        output
            .deployed_addresses
            .bridgehub
            .ctm_deployment_tracker_proxy_addr,
    );
    // This is force deployment data for creating new contracts, not really relevant here tbh,
    contracts_config.ecosystem_contracts.force_deployments_data = Some(hex::encode(
        &output.contracts_config.force_deployments_data.0,
    ));
    contracts_config.ecosystem_contracts.native_token_vault_addr =
        Some(output.deployed_addresses.native_token_vault_addr);
    contracts_config
        .ecosystem_contracts
        .l1_bytecodes_supplier_addr = Some(output.deployed_addresses.l1_bytecodes_supplier_addr);

    contracts_config.l1.rollup_l1_da_validator_addr =
        Some(output.deployed_addresses.rollup_l1_da_validator_addr);

    contracts_config.l1.no_da_validium_l1_validator_addr =
        Some(output.deployed_addresses.validium_l1_da_validator_addr);
}

// Governance has approved the proposal, now it will insert the new protocol version into our STM (CTM)
async fn governance_stage_2(
    init_args: &mut EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let spinner = Spinner::new("Executing governance stage 2!");

    let previous_output =
        EcosystemUpgradeOutput::read_with_base_path(shell, &ecosystem_config.config)?;

    // These are ABI-encoded
    let stage2_calls = previous_output.governance_calls.stage2_calls;
    let l1_rpc_url = if let Some(url) = init_args.l1_rpc_url.clone() {
        url
    } else {
        ecosystem_config
            .load_current_chain()?
            .get_secrets_config()
            .await?
            .l1_rpc_url()?
    };

    governance_execute_calls(
        shell,
        ecosystem_config,
        &ecosystem_config.get_wallets()?.governor,
        stage2_calls.0,
        &init_args.forge_args.clone(),
        l1_rpc_url.clone(),
    )
    .await?;

    let mut contracts_config = ecosystem_config.get_contracts_config()?;
    contracts_config.bridges.shared.l1_address = previous_output
        .deployed_addresses
        .bridges
        .shared_bridge_proxy_addr;

    contracts_config.save_with_base_path(shell, &ecosystem_config.config)?;
    spinner.finish();

    Ok(())
}

lazy_static! {
    static ref FINALIZE_UPGRADE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function initChains(address bridgehub, uint256[] chains) public",
            "function initTokens(address l1NativeTokenVault, address[] tokens, uint256[] chains) public",
        ])
        .unwrap(),
    );
}

// Governance has approved the proposal, now it will insert the new protocol version into our STM (CTM)
async fn no_governance_stage_2(
    init_args: &mut EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let contracts_config = ecosystem_config.get_contracts_config()?;
    let wallets = ecosystem_config.get_wallets()?;
    let deployer_private_key = wallets
        .deployer
        .context("deployer_wallet")?
        .private_key_h256()
        .context("deployer_priuvate_key")?;

    let spinner = Spinner::new("Finalizing stage2 of the upgrade");

    let chains: Vec<_> = ecosystem_config
        .list_of_chains()
        .into_iter()
        .filter_map(|name| {
            let chain = ecosystem_config
                .load_chain(Some(name))
                .expect("Invalid chain");
            (chain.name != "gateway").then_some(chain)
        })
        .collect();

    let l1_rpc_url = if let Some(url) = init_args.l1_rpc_url.clone() {
        url
    } else {
        chains
            .first()
            .unwrap()
            .get_secrets_config()
            .await?
            .l1_rpc_url()?
    };
    let chain_ids: Vec<_> = chains
        .into_iter()
        .map(|c| ethers::abi::Token::Uint(U256::from(c.chain_id.as_u64())))
        .collect();
    let mut tokens: Vec<_> = ecosystem_config
        .get_erc20_tokens()
        .into_iter()
        .map(|t| ethers::abi::Token::Address(t.address))
        .collect();
    tokens.push(ethers::abi::Token::Address(
        SHARED_BRIDGE_ETHER_TOKEN_ADDRESS,
    ));

    // Resume for accept admin doesn't work properly. Foundry assumes that if signature of the function is the same,
    // than it's the same call, but because we are calling this function multiple times during the init process,
    // code assumes that doing only once is enough, but actually we need to accept admin multiple times
    let mut forge_args = init_args.forge_args.clone();
    forge_args.resume = false;

    let init_chains_calldata = FINALIZE_UPGRADE
        .encode(
            "initChains",
            (
                ethers::abi::Token::Address(
                    contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
                ),
                ethers::abi::Token::Array(chain_ids.clone()),
            ),
        )
        .unwrap();
    let init_tokens_calldata = FINALIZE_UPGRADE
        .encode(
            "initTokens",
            (
                ethers::abi::Token::Address(
                    contracts_config
                        .ecosystem_contracts
                        .native_token_vault_addr
                        .context("native_token_vault_addr")?,
                ),
                ethers::abi::Token::Array(tokens),
                ethers::abi::Token::Array(chain_ids),
            ),
        )
        .unwrap();

    logger::info("Initiing chains!");
    let foundry_contracts_path = ecosystem_config.path_to_l1_foundry();
    let forge = Forge::new(&foundry_contracts_path)
        .script(&FINALIZE_UPGRADE_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url.clone())
        .with_broadcast()
        .with_calldata(&init_chains_calldata)
        .with_private_key(deployer_private_key);

    forge.run(shell)?;

    logger::info("Initiing tokens!");

    let forge = Forge::new(&foundry_contracts_path)
        .script(&FINALIZE_UPGRADE_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&init_tokens_calldata)
        .with_private_key(deployer_private_key);

    forge.run(shell)?;

    spinner.finish();

    Ok(())
}

fn get_ecosystem_upgrade_params(upgrade_version: &UpgradeVersions) -> ForgeScriptParams {
    match upgrade_version {
        UpgradeVersions::V28_1Vk => ZK_OS_V28_1_UPGRADE_ECOSYSTEM_PARAMS,
        UpgradeVersions::V29InteropAFf => V29_UPGRADE_ECOSYSTEM_PARAMS,
    }
}
