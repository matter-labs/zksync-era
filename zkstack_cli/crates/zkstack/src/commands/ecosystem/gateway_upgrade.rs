use anyhow::Context;
use common::{forge::Forge, git, spinner::Spinner};
use config::{
    forge_interface::{
        gateway_ecosystem_upgrade::{
            input::GatewayEcosystemUpgradeInput, output::GatewayEcosystemUpgradeOutput,
        },
        script_params::{FINALIZE_UPGRADE_SCRIPT_PARAMS, GATEWAY_UPGRADE_ECOSYSTEM_PARAMS},
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath},
    EcosystemConfig, GenesisConfig, CONFIGS_PATH,
};
use ethers::{abi::parse_abi, contract::BaseContract, utils::hex};
use lazy_static::lazy_static;
use types::ProverMode;
use xshell::Shell;
use zksync_types::{H160, L2_NATIVE_TOKEN_VAULT_ADDRESS, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS, U256};

use super::args::gateway_upgrade::{GatewayUpgradeArgs, GatewayUpgradeArgsFinal};
use crate::{
    accept_ownership::governance_execute_calls,
    commands::ecosystem::args::gateway_upgrade::GatewayUpgradeStage,
    messages::MSG_INTALLING_DEPS_SPINNER,
    utils::forge::{fill_forge_private_key, WalletOwner},
};

pub async fn run(args: GatewayUpgradeArgs, shell: &Shell) -> anyhow::Result<()> {
    println!("Running ecosystem gateway upgrade args");

    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let mut final_ecosystem_args = args.fill_values_with_prompt(ecosystem_config.l1_network, true);

    match final_ecosystem_args.ecosystem_upgrade_stage {
        GatewayUpgradeStage::NoGovernancePrepare => {
            no_governance_prepare(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
        GatewayUpgradeStage::GovernanceStage1 => {
            governance_stage_1(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
        GatewayUpgradeStage::GovernanceStage2 => {
            governance_stage_2(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
        GatewayUpgradeStage::NoGovernanceStage2 => {
            no_governance_stage_2(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
    }

    Ok(())
}

async fn no_governance_prepare(
    init_args: &mut GatewayUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let forge_args = init_args.forge_args.clone();
    let l1_rpc_url = init_args.l1_rpc_url.clone();

    let new_genesis_config = GenesisConfig::read_with_base_path(shell, CONFIGS_PATH)?;
    let current_contracts_config = ecosystem_config.get_contracts_config()?;
    let initial_deployment_config = ecosystem_config.get_initial_deployment_config()?;
    let wallets_config = ecosystem_config.get_wallets()?;

    let ecosystem_upgrade_config_path =
        GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.input(&ecosystem_config.link_to_code);

    let era_config = ecosystem_config
        .load_chain(Some("era".to_string()))
        .context("No era")?;

    // FIXME: we will have to force this in production environment
    // assert_eq!(era_config.chain_id, ecosystem_config.era_chain_id);

    let gateway_upgrade_input = GatewayEcosystemUpgradeInput::new(
        &new_genesis_config,
        &current_contracts_config,
        &initial_deployment_config,
        &wallets_config,
        ecosystem_config.era_chain_id,
        // FIXME: provide correct era diamond proxy
        era_config.get_contracts_config()?.l1.diamond_proxy_addr,
        ecosystem_config.prover_version == ProverMode::NoProofs,
    );
    gateway_upgrade_input.save(shell, ecosystem_upgrade_config_path.clone())?;

    let mut forge = Forge::new(&ecosystem_config.path_to_l1_foundry())
        .script(
            &GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.script(),
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

    println!("Preparing the ecosystem for the upgrade!");

    forge.run(shell)?;

    println!("done!");

    let output = GatewayEcosystemUpgradeOutput::read(
        shell,
        GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.output(&ecosystem_config.link_to_code),
    )?;
    output.save_with_base_path(shell, &ecosystem_config.config)?;

    Ok(())
}

// Governance has approved the proposal, now it will insert the new protocol version into our STM (CTM)
async fn governance_stage_1(
    init_args: &mut GatewayUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    println!("Executing governance stage 1!");

    let previous_output = GatewayEcosystemUpgradeOutput::read(
        shell,
        GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.output(&ecosystem_config.link_to_code),
    )?;
    previous_output.save_with_base_path(shell, &ecosystem_config.config)?;

    // These are ABI-encoded
    let stage1_calls = previous_output.governance_stage1_calls;

    governance_execute_calls(
        shell,
        ecosystem_config,
        &ecosystem_config.get_wallets()?.governor,
        stage1_calls.0,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;

    let gateway_ecosystem_preparation_output =
        GatewayEcosystemUpgradeOutput::read_with_base_path(shell, &ecosystem_config.config)?;

    let mut contracts_config = ecosystem_config.get_contracts_config()?;

    contracts_config
        .ecosystem_contracts
        .stm_deployment_tracker_proxy_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .bridgehub
            .ctm_deployment_tracker_proxy_addr,
    );
    // This is force deployment data for creating new contracts, not really relevant here tbh,
    contracts_config.ecosystem_contracts.force_deployments_data = Some(hex::encode(
        &gateway_ecosystem_preparation_output
            .contracts_config
            .force_deployments_data
            .0,
    ));
    contracts_config.ecosystem_contracts.native_token_vault_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .native_token_vault_addr,
    );
    contracts_config
        .ecosystem_contracts
        .l1_bytecodes_supplier_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .l1_bytecodes_supplier_addr,
    );

    contracts_config.l1.rollup_l1_da_validator_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .rollup_l1_da_validator_addr,
    );
    contracts_config.l1.validium_l1_da_validator_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .validium_l1_da_validator_addr,
    );

    // This value is meaningless for the ecosystem, but we'll populate it for consistency
    contracts_config.l2.da_validator_addr = Some(H160::zero());
    contracts_config.l2.l2_native_token_vault_proxy_addr = Some(L2_NATIVE_TOKEN_VAULT_ADDRESS);
    contracts_config.l2.legacy_shared_bridge_addr = contracts_config.bridges.shared.l2_address;

    contracts_config.save_with_base_path(shell, &ecosystem_config.config)?;

    Ok(())
}

// Governance has approved the proposal, now it will insert the new protocol version into our STM (CTM)
async fn governance_stage_2(
    init_args: &mut GatewayUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    println!("Executing governance stage 2!");

    let previous_output =
        GatewayEcosystemUpgradeOutput::read_with_base_path(shell, &ecosystem_config.config)?;

    // These are ABI-encoded
    let stage2_calls = previous_output.governance_stage2_calls;

    governance_execute_calls(
        shell,
        ecosystem_config,
        &ecosystem_config.get_wallets()?.governor,
        stage2_calls.0,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;

    let mut contracts_config = ecosystem_config.get_contracts_config()?;
    contracts_config.bridges.shared.l1_address = previous_output
        .deployed_addresses
        .bridges
        .shared_bridge_proxy_addr;

    contracts_config.save_with_base_path(shell, &ecosystem_config.config)?;
    println!("Stage2 finalized!");

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
    init_args: &mut GatewayUpgradeArgsFinal,
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

    println!("Finalizing stage2 of the upgrade!");

    let chains: Vec<_> = ecosystem_config
        .list_of_chains()
        .into_iter()
        .map(|name| {
            ecosystem_config
                .load_chain(Some(name))
                .expect("Invalid chain")
        })
        .collect();

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

    println!("Initiing chains!");
    let foundry_contracts_path = ecosystem_config.path_to_l1_foundry();
    let forge = Forge::new(&foundry_contracts_path)
        .script(&FINALIZE_UPGRADE_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(init_args.l1_rpc_url.clone())
        .with_broadcast()
        .with_calldata(&init_chains_calldata)
        .with_private_key(deployer_private_key);

    forge.run(shell)?;

    println!("Initiing tokens!");

    let forge = Forge::new(&foundry_contracts_path)
        .script(&FINALIZE_UPGRADE_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(init_args.l1_rpc_url.clone())
        .with_broadcast()
        .with_calldata(&init_tokens_calldata)
        .with_private_key(deployer_private_key);

    forge.run(shell)?;

    println!("Done!");

    Ok(())
}
