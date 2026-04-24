use std::path::{Path, PathBuf};

use anyhow::Context;
use ethers::{
    abi::{Function, Param, ParamType, StateMutability, Token},
    contract::BaseContract,
    types::{Address, Bytes},
    utils::hex,
};
use lazy_static::lazy_static;
use serde::Deserialize;
use xshell::Shell;
use zkstack_cli_common::{forge::Forge, logger, spinner::Spinner};
use zkstack_cli_config::{
    forge_interface::{
        script_params::{
            ForgeScriptParams, FINALIZE_UPGRADE_SCRIPT_PARAMS, V29_UPGRADE_ECOSYSTEM_PARAMS,
            V31_UPGRADE_ECOSYSTEM_PARAMS,
        },
        upgrade_ecosystem::output::EcosystemUpgradeOutput,
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfigWithBasePath},
    CoreContractsConfig, EcosystemConfig, ZkStackConfig,
};
use zkstack_cli_types::VMOption;
use zksync_types::{SHARED_BRIDGE_ETHER_TOKEN_ADDRESS, U256};

use crate::{
    abi::IFINALIZEUPGRADEABI_ABI,
    admin_functions::{governance_execute_calls, AdminScriptMode},
    commands::dev::commands::upgrades::{
        args::ecosystem::{EcosystemUpgradeArgs, EcosystemUpgradeArgsFinal, EcosystemUpgradeStage},
        types::UpgradeVersion,
    },
    messages::MSG_INTALLING_DEPS_SPINNER,
    utils::forge::{fill_forge_private_key, WalletOwner},
};

pub async fn run(
    shell: &Shell,
    args: EcosystemUpgradeArgs,
    run_upgrade: bool,
) -> anyhow::Result<()> {
    let vm_option = args.common.vm_option();

    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    let upgrade_version = args.upgrade_version;

    let final_ecosystem_args = args.fill_values_with_prompt(run_upgrade);

    match final_ecosystem_args.ecosystem_upgrade_stage {
        EcosystemUpgradeStage::NoGovernancePrepare => {
            no_governance_prepare(
                &final_ecosystem_args,
                shell,
                &ecosystem_config,
                &upgrade_version,
                vm_option,
            )
            .await?;
        }
        EcosystemUpgradeStage::EcosystemAdmin => {
            ecosystem_admin(
                &final_ecosystem_args,
                shell,
                &ecosystem_config,
                &upgrade_version,
                vm_option,
            )
            .await?;
        }
        EcosystemUpgradeStage::GovernanceStage0 => {
            governance_stage_0(
                &final_ecosystem_args,
                shell,
                &ecosystem_config,
                &upgrade_version,
                vm_option,
            )
            .await?;
        }
        EcosystemUpgradeStage::GovernanceStage1 => {
            governance_stage_1(
                &final_ecosystem_args,
                shell,
                &ecosystem_config,
                &upgrade_version,
                vm_option,
            )
            .await?;
        }
        EcosystemUpgradeStage::GovernanceStage2 => {
            governance_stage_2(&final_ecosystem_args, shell, &ecosystem_config, vm_option).await?;
        }
        EcosystemUpgradeStage::NoGovernanceStage2 => {
            no_governance_stage_2(&final_ecosystem_args, shell, &ecosystem_config, vm_option)
                .await?;
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
    init_args: &EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    upgrade_version: &UpgradeVersion,
    vm_option: VMOption,
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

    let forge_calldata = match upgrade_version {
        UpgradeVersion::V31InteropB => Some(build_v31_no_governance_prepare_calldata(
            ecosystem_config,
            vm_option,
        )?),
        _ => None,
    };

    let mut forge = Forge::new(&ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option))
        .script(
            &get_ecosystem_upgrade_params(upgrade_version).script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_slow()
        .with_gas_limit(1_000_000_000_000)
        .with_broadcast();

    if let Some(calldata) = forge_calldata.as_ref() {
        forge = forge.with_calldata(calldata);
    }

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.deployer.as_ref(),
        WalletOwner::Deployer,
    )?;

    logger::info("Preparing the ecosystem for the upgrade!".to_string());

    forge.run(shell)?;

    logger::info("done!");

    let l1_chain_id = ecosystem_config.l1_network.chain_id();
    let broadcast_path = get_broadcast_path(
        &ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option),
        get_ecosystem_upgrade_params(upgrade_version).script(),
        l1_chain_id,
        forge_calldata.as_ref(),
    )?;

    // TODO Get rid of BrodacastFile usage
    let broadcast_file: BroadcastFile = {
        let file_content = std::fs::read_to_string(&broadcast_path).with_context(|| {
            format!("Failed to read broadcast file {}", broadcast_path.display())
        })?;
        serde_json::from_str(&file_content).context("Failed to parse broadcast file")?
    };

    let mut output = EcosystemUpgradeOutput::read(
        shell,
        get_ecosystem_upgrade_params(upgrade_version)
            .output(&ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option)),
    )?;

    // Add all the transaction hashes.
    for tx in broadcast_file.transactions {
        output.transactions.push(tx.hash);
    }

    output.save_with_base_path(shell, &ecosystem_config.config)?;

    Ok(())
}

fn build_v31_no_governance_prepare_calldata(
    ecosystem_config: &EcosystemConfig,
    vm_option: VMOption,
) -> anyhow::Result<Bytes> {
    let contracts_config = ecosystem_config.get_contracts_config()?;
    let ctm = contracts_config.ctm(vm_option);
    let governance = contracts_config.l1.governance_addr;
    let zk_token_asset_id = ecosystem_config.l1_network.zk_token_asset_id();
    #[allow(deprecated)]
    let ecosystem_upgrade_v31 = Function {
        name: "noGovernancePrepare".to_string(),
        inputs: vec![Param {
            name: "params".to_string(),
            kind: ParamType::Tuple(vec![
                ParamType::Address,
                ParamType::Address,
                ParamType::Address,
                ParamType::Address,
                ParamType::Bool,
                ParamType::FixedBytes(32),
                ParamType::String,
                ParamType::String,
                ParamType::Address,
                ParamType::FixedBytes(32),
            ]),
            internal_type: Some("struct EcosystemUpgradeParams".to_string()),
        }],
        outputs: vec![],
        constant: None,
        state_mutability: StateMutability::NonPayable,
    };

    let calldata = ecosystem_upgrade_v31
        .encode_input(&[Token::Tuple(vec![
            Token::Address(
                contracts_config
                    .core_ecosystem_contracts
                    .bridgehub_proxy_addr,
            ),
            Token::Address(ctm.state_transition_proxy_addr),
            Token::Address(ctm.l1_bytecodes_supplier_addr),
            Token::Address(ctm.l1_rollup_da_manager),
            Token::Bool(matches!(vm_option, VMOption::ZKSyncOsVM)),
            Token::FixedBytes(contracts_config.create2_factory_salt.as_bytes().to_vec()),
            Token::String("/upgrade-envs/v0.31.0-interopB/local.toml".to_string()),
            Token::String("/script-out/v31-upgrade-ecosystem.toml".to_string()),
            Token::Address(governance),
            Token::FixedBytes(zk_token_asset_id.as_bytes().to_vec()),
        ])])
        .context("Failed to encode v31 no-governance-prepare calldata")?;

    Ok(Bytes::from(calldata))
}

fn get_broadcast_path(
    foundry_root: &Path,
    script_path: impl AsRef<Path>,
    l1_chain_id: u64,
    forge_calldata: Option<&Bytes>,
) -> anyhow::Result<PathBuf> {
    let script_name = script_path
        .as_ref()
        .file_name()
        .context("Missing script filename")?;
    let broadcast_dir = foundry_root
        .join("broadcast")
        .join(script_name)
        .join(l1_chain_id.to_string());

    let filename = match forge_calldata {
        Some(calldata) => {
            let selector = calldata
                .0
                .get(..4)
                .context("forge calldata must include a 4-byte function selector")?;
            format!("{}-latest.json", hex::encode(selector))
        }
        None => "run-latest.json".to_string(),
    };
    let path = broadcast_dir.join(&filename);
    anyhow::ensure!(
        path.exists(),
        "Expected Foundry broadcast file at {}",
        path.display()
    );
    Ok(path)
}

async fn ecosystem_admin(
    _init_args: &EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    upgrade_version: &UpgradeVersion,
    vm_option: VMOption,
) -> anyhow::Result<()> {
    let spinner = Spinner::new("Executing ecosystem admin!");

    let previous_output = EcosystemUpgradeOutput::read(
        shell,
        get_ecosystem_upgrade_params(upgrade_version)
            .output(&ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option)),
    )?;
    previous_output.save_with_base_path(shell, &ecosystem_config.config)?;
    spinner.finish();

    Ok(())
}

async fn governance_stage_0(
    init_args: &EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    upgrade_version: &UpgradeVersion,
    vm_option: VMOption,
) -> anyhow::Result<()> {
    let spinner = Spinner::new("Executing governance stage 0!");

    let previous_output = EcosystemUpgradeOutput::read(
        shell,
        get_ecosystem_upgrade_params(upgrade_version)
            .output(&ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option)),
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
        ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option),
        AdminScriptMode::Broadcast(ecosystem_config.get_wallets()?.governor),
        stage0_calls.0,
        &init_args.forge_args.clone(),
        l1_rpc_url,
        ecosystem_config.get_contracts_config()?.l1.governance_addr,
    )
    .await?;
    spinner.finish();

    Ok(())
}

// Governance has approved the proposal, now it will insert the new protocol version into our STM (CTM)
async fn governance_stage_1(
    init_args: &EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    upgrade_version: &UpgradeVersion,
    vm_option: VMOption,
) -> anyhow::Result<()> {
    let previous_output = EcosystemUpgradeOutput::read(
        shell,
        get_ecosystem_upgrade_params(upgrade_version)
            .output(&ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option)),
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
        ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option),
        AdminScriptMode::Broadcast(ecosystem_config.get_wallets()?.governor),
        stage1_calls.0,
        &init_args.forge_args.clone(),
        l1_rpc_url.clone(),
        ecosystem_config.get_contracts_config()?.l1.governance_addr,
    )
    .await?;

    let gateway_ecosystem_preparation_output =
        EcosystemUpgradeOutput::read_with_base_path(shell, &ecosystem_config.config)?;

    let mut contracts_config = ecosystem_config.get_contracts_config()?;

    update_contracts_config_from_output(
        &mut contracts_config,
        &gateway_ecosystem_preparation_output,
        vm_option,
    );

    contracts_config.save_with_base_path(shell, &ecosystem_config.config)?;

    Ok(())
}

fn update_contracts_config_from_output(
    contracts_config: &mut CoreContractsConfig,
    output: &EcosystemUpgradeOutput,
    vm_option: VMOption,
) {
    // Update the BytecodesSupplier address in the CTM config if it's present in the upgrade output
    if let Some(ref state_transition) = output.state_transition {
        if state_transition.bytecodes_supplier_addr != Address::zero() {
            let ctm = match vm_option {
                VMOption::EraVM => contracts_config.era_ctm.as_mut(),
                VMOption::ZKSyncOsVM => contracts_config.zksync_os_ctm.as_mut(),
            };

            if let Some(ctm) = ctm {
                ctm.l1_bytecodes_supplier_addr = state_transition.bytecodes_supplier_addr;
                logger::info(format!(
                    "Updated BytecodesSupplier address in CTM config to: {:?}",
                    state_transition.bytecodes_supplier_addr
                ));
            }
        }
    }
}

// Governance has approved the proposal, now it will insert the new protocol version into our STM (CTM)
async fn governance_stage_2(
    init_args: &EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    vm_option: VMOption,
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
        ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option),
        AdminScriptMode::Broadcast(ecosystem_config.get_wallets()?.governor),
        stage2_calls.0,
        &init_args.forge_args.clone(),
        l1_rpc_url.clone(),
        ecosystem_config.get_contracts_config()?.l1.governance_addr,
    )
    .await?;

    spinner.finish();

    Ok(())
}

lazy_static! {
    static ref FINALIZE_UPGRADE: BaseContract = BaseContract::from(IFINALIZEUPGRADEABI_ABI.clone());
}

// Governance has approved the proposal, now it will insert the new protocol version into our STM (CTM)
// TODO: maybe delete the file?
async fn no_governance_stage_2(
    init_args: &EcosystemUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    vm_option: VMOption,
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
                    contracts_config
                        .core_ecosystem_contracts
                        .bridgehub_proxy_addr,
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
                        .core_ecosystem_contracts
                        .native_token_vault_addr
                        .context("native_token_vault_addr")?,
                ),
                ethers::abi::Token::Array(tokens),
                ethers::abi::Token::Array(chain_ids),
            ),
        )
        .unwrap();

    logger::info("Initiing chains!");
    let foundry_contracts_path = ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option);
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

fn get_ecosystem_upgrade_params(upgrade_version: &UpgradeVersion) -> ForgeScriptParams {
    match upgrade_version {
        UpgradeVersion::V29InteropAFf => V29_UPGRADE_ECOSYSTEM_PARAMS,
        UpgradeVersion::V29_3 => unreachable!("V29_3 does not support ecosystem upgrade"),
        UpgradeVersion::V29_4 => unreachable!("V29_4 does not support ecosystem upgrade"),
        UpgradeVersion::V31InteropB => V31_UPGRADE_ECOSYSTEM_PARAMS,
    }
}
