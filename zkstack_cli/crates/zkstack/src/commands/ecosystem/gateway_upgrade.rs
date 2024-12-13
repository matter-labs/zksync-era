use anyhow::Context;
use common::{db::DatabaseConfig, forge::Forge, git, spinner::Spinner};
use config::{
    forge_interface::{
        gateway_ecosystem_upgrade::{
            input::GatewayEcosystemUpgradeInput, output::GatewayEcosystemUpgradeOutput,
        },
        gateway_preparation::input::GatewayPreparationConfig,
        script_params::{
            FINALIZE_UPGRADE_SCRIPT_PARAMS, GATEWAY_GOVERNANCE_TX_PATH1, GATEWAY_PREPARATION,
            GATEWAY_UPGRADE_ECOSYSTEM_PARAMS,
        },
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath},
    EcosystemConfig, GenesisConfig, CONFIGS_PATH,
};
use ethers::{abi::parse_abi, contract::BaseContract, utils::hex};
use lazy_static::lazy_static;
use types::{BaseToken, ProverMode, WalletCreation};
use xshell::Shell;
use zksync_basic_types::commitment::L1BatchCommitmentMode;
use zksync_types::{
    Address, H160, L2_NATIVE_TOKEN_VAULT_ADDRESS, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS, U256,
};

use super::args::gateway_upgrade::{GatewayUpgradeArgs, GatewayUpgradeArgsFinal};
use crate::{
    accept_ownership::{
        accept_admin, governance_execute_calls, make_permanent_rollup, set_da_validator_pair,
    },
    commands::{
        chain,
        chain::{
            args::{
                genesis::GenesisArgsFinal,
                init::{configs::InitConfigsArgsFinal, da_configs::ValidiumType},
            },
            convert_to_gateway::{
                calculate_gateway_ctm, call_script, GATEWAY_PREPARATION_INTERFACE,
            },
            deploy_l2_contracts,
            genesis::genesis,
            init::configs::init_configs,
            register_chain::register_chain,
            ChainCreateArgsFinal,
        },
        ecosystem::args::gateway_upgrade::GatewayUpgradeStage,
    },
    defaults::{generate_db_names, DBNames, DATABASE_SERVER_URL},
    messages::{MSG_CHAIN_NOT_FOUND_ERR, MSG_GENESIS_DATABASE_ERR, MSG_INTALLING_DEPS_SPINNER},
    utils::forge::{fill_forge_private_key, WalletOwner},
};

pub async fn run(args: GatewayUpgradeArgs, shell: &Shell) -> anyhow::Result<()> {
    println!("Running ecosystem gateway upgrade args");

    let mut ecosystem_config = EcosystemConfig::from_file(shell)?;
    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let mut final_ecosystem_args = args.fill_values_with_prompt(ecosystem_config.l1_network, true);

    match final_ecosystem_args.ecosystem_upgrade_stage {
        GatewayUpgradeStage::NoGovernancePrepare => {
            no_governance_prepare(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
            no_governance_prepare_gateway(&mut final_ecosystem_args, shell, &mut ecosystem_config)
                .await?;
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
        GatewayUpgradeStage::GovernanceStage3 => {
            governance_stage_3(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
        }
        GatewayUpgradeStage::NoGovernanceStage3 => {
            no_governance_stage_3(&mut final_ecosystem_args, shell, &ecosystem_config).await?;
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

async fn no_governance_prepare_gateway(
    init_args: &mut GatewayUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &mut EcosystemConfig,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let forge_args = init_args.forge_args.clone();
    let l1_rpc_url = init_args.l1_rpc_url.clone();

    let mut contracts_config = ecosystem_config.get_contracts_config()?;

    {
        let output = GatewayEcosystemUpgradeOutput::read(
            shell,
            GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.output(&ecosystem_config.link_to_code),
        )?;

        let mut s: String = "0x".to_string();
        s += &hex::encode(output.contracts_config.diamond_cut_data.0);
        contracts_config.ecosystem_contracts.diamond_cut_data = s;

        s = "0x".to_string();
        s += &hex::encode(output.contracts_config.force_deployments_data.0);
        contracts_config.ecosystem_contracts.force_deployments_data = Some(s);

        contracts_config.l1.rollup_l1_da_validator_addr =
            Some(output.deployed_addresses.rollup_l1_da_validator_addr);
        contracts_config.l1.no_da_validium_l1_validator_addr =
            Some(output.deployed_addresses.validium_l1_da_validator_addr);

        contracts_config
            .ecosystem_contracts
            .stm_deployment_tracker_proxy_addr = Some(
            output
                .deployed_addresses
                .bridgehub
                .ctm_deployment_tracker_proxy_addr,
        );
        contracts_config.ecosystem_contracts.native_token_vault_addr =
            Some(output.deployed_addresses.native_token_vault_addr);
        contracts_config
            .ecosystem_contracts
            .l1_bytecodes_supplier_addr =
            Some(output.deployed_addresses.l1_bytecodes_supplier_addr);
        contracts_config.bridges.l1_nullifier_addr =
            Some(contracts_config.bridges.shared.l1_address);
        contracts_config.ecosystem_contracts.validator_timelock_addr =
            output.deployed_addresses.validator_timelock_addr;
        contracts_config.l1.validator_timelock_addr =
            output.deployed_addresses.validator_timelock_addr;
        contracts_config.bridges.shared.l1_address =
            output.deployed_addresses.bridges.shared_bridge_proxy_addr;
        contracts_config
            .ecosystem_contracts
            .expected_rollup_l2_da_validator =
            Some(output.contracts_config.expected_rollup_l2_da_validator);
    }

    let chain_create_args = ChainCreateArgsFinal {
        chain_name: "gateway".to_string(),
        chain_id: 505,
        prover_version: ProverMode::NoProofs,
        wallet_creation: WalletCreation::Localhost,
        l1_batch_commit_data_generator_mode: L1BatchCommitmentMode::Rollup,
        wallet_path: None,
        base_token: BaseToken::eth(),
        set_as_default: false,
        legacy_bridge: false,
        skip_submodules_checkout: false,
        skip_contract_compilation_override: false,
        evm_emulator: false,
        link_to_code: ecosystem_config.link_to_code.clone().display().to_string(),
    };
    chain::create::create_chain_inner(chain_create_args, ecosystem_config, shell)?;
    let chain_config = ecosystem_config
        .load_chain(Some("gateway".to_string()))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    register_chain(
        shell,
        forge_args,
        ecosystem_config,
        &chain_config,
        &mut contracts_config,
        l1_rpc_url.clone(),
        None,
        false,
    )
    .await?;
    shell.copy_file(
        ecosystem_config.link_to_code.join(
            "contracts/l1-contracts/broadcast/RegisterZKChain.s.sol/9/dry-run/run-latest.json",
        ),
        ecosystem_config
            .link_to_code
            .join(GATEWAY_GOVERNANCE_TX_PATH1),
    )?;

    let DBNames { server_name, .. } = generate_db_names(&chain_config);
    let args = InitConfigsArgsFinal {
        genesis_args: GenesisArgsFinal {
            server_db: DatabaseConfig::new(DATABASE_SERVER_URL.clone(), server_name),
            dont_drop: false,
        },
        l1_rpc_url,
        no_port_reallocation: false,
        validium_config: Some(ValidiumType::NoDA),
    };
    init_configs(&args, shell, ecosystem_config, &chain_config).await?;

    deploy_l2_contracts::deploy_l2_contracts(
        shell,
        &chain_config,
        ecosystem_config,
        &mut contracts_config,
        init_args.forge_args.clone(),
        false,
    )
    .await?;
    shell.copy_file(
        ecosystem_config.link_to_code.join(
            "contracts/l1-contracts/broadcast/DeployL2Contracts.sol/9/dry-run/run-latest.json",
        ),
        ecosystem_config
            .link_to_code
            .join(GATEWAY_GOVERNANCE_TX_PATH1),
    )?;

    contracts_config.l1.base_token_addr = Address::from_low_u64_be(1);
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    //==========

    let chain_genesis_config = chain_config.get_genesis_config()?;
    let chain_contracts_config = chain_config.get_contracts_config()?;
    let gateway_config = chain::convert_to_gateway::calculate_gateway_ctm(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        &chain_config,
        &chain_genesis_config,
        &ecosystem_config.get_initial_deployment_config().unwrap(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;

    // =========

    let gateway_preparation_config_path = GATEWAY_PREPARATION.input(&chain_config.link_to_code);
    let preparation_config = GatewayPreparationConfig::new(
        &chain_config,
        &chain_contracts_config,
        &ecosystem_config.get_contracts_config()?,
        &gateway_config,
    )?;
    preparation_config.save(shell, gateway_preparation_config_path)?;

    chain::convert_to_gateway::gateway_governance_whitelisting(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        &chain_config,
        gateway_config,
        init_args.l1_rpc_url.clone(),
        false,
    )
    .await?;

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

    contracts_config.l1.no_da_validium_l1_validator_addr = Some(
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
        .filter_map(|name| {
            let chain = ecosystem_config
                .load_chain(Some(name))
                .expect("Invalid chain");
            (chain.name != "gateway").then_some(chain)
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

async fn governance_stage_3(
    init_args: &mut GatewayUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let chain_config = ecosystem_config
        .load_chain(Some("gateway".to_string()))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    call_script(
        shell,
        init_args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode("executeGovernanceTxs", ())
            .unwrap(),
        ecosystem_config,
        &chain_config,
        &ecosystem_config.get_wallets()?.governor,
        init_args.l1_rpc_url.clone(),
        true,
    )
    .await?;

    Ok(())
}

async fn no_governance_stage_3(
    init_args: &mut GatewayUpgradeArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let chain_config = ecosystem_config
        .load_chain(Some("gateway".to_string()))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let chain_genesis_config = chain_config.get_genesis_config()?;
    let mut chain_contracts_config = chain_config.get_contracts_config()?;

    // Fund gateway's governor (chain_config.get_wallets_config()?.governor)
    chain::common::distribute_eth(
        ecosystem_config,
        &chain_config,
        init_args.l1_rpc_url.clone(),
    )
    .await?;

    // Accept ownership for DiamondProxy (run by L2 Governor)
    accept_admin(
        shell,
        ecosystem_config,
        chain_contracts_config.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        chain_contracts_config.l1.diamond_proxy_addr,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;

    // prepare script input
    let gateway_config = calculate_gateway_ctm(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        &chain_config,
        &chain_genesis_config,
        &ecosystem_config.get_initial_deployment_config().unwrap(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;

    let gateway_preparation_config_path = GATEWAY_PREPARATION.input(&chain_config.link_to_code);
    let preparation_config = GatewayPreparationConfig::new(
        &chain_config,
        &chain_contracts_config,
        &ecosystem_config.get_contracts_config()?,
        &gateway_config,
    )?;
    preparation_config.save(shell, gateway_preparation_config_path)?;

    // deploy filterer
    let output = call_script(
        shell,
        init_args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode("deployAndSetGatewayTransactionFilterer", ())
            .unwrap(),
        ecosystem_config,
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        init_args.l1_rpc_url.clone(),
        true,
    )
    .await?;

    chain_contracts_config.set_transaction_filterer(output.gateway_transaction_filterer_proxy);

    // whitelist deployer
    call_script(
        shell,
        init_args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "grantWhitelist",
                (
                    output.gateway_transaction_filterer_proxy,
                    vec![
                        ecosystem_config.get_contracts_config()?.l1.governance_addr,
                        ecosystem_config
                            .get_wallets()?
                            .deployer
                            .context("no deployer addr")?
                            .address,
                    ],
                ),
            )
            .unwrap(),
        ecosystem_config,
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        init_args.l1_rpc_url.clone(),
        true,
    )
    .await?;

    // deploy ctm
    chain::convert_to_gateway::deploy_gateway_ctm(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        &chain_config,
        &chain_genesis_config,
        &ecosystem_config.get_initial_deployment_config().unwrap(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;

    chain_contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    // Set da validators
    let validium_mode =
        chain_config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Validium;
    let l1_da_validator_addr = if validium_mode {
        chain_contracts_config.l1.no_da_validium_l1_validator_addr
    } else {
        chain_contracts_config.l1.rollup_l1_da_validator_addr
    };
    set_da_validator_pair(
        shell,
        ecosystem_config,
        chain_contracts_config.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        chain_contracts_config.l1.diamond_proxy_addr,
        l1_da_validator_addr.context("l1_da_validator_addr")?,
        chain_contracts_config
            .l2
            .da_validator_addr
            .context("da_validator_addr")?,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    if !validium_mode {
        make_permanent_rollup(
            shell,
            ecosystem_config,
            chain_contracts_config.l1.chain_admin_addr,
            &chain_config.get_wallets_config()?.governor,
            chain_contracts_config.l1.diamond_proxy_addr,
            &init_args.forge_args.clone(),
            init_args.l1_rpc_url.clone(),
        )
        .await?;
    }

    let DBNames { server_name, .. } = generate_db_names(&chain_config);
    let args = GenesisArgsFinal {
        server_db: DatabaseConfig::new(DATABASE_SERVER_URL.clone(), server_name),
        dont_drop: false,
    };
    // Run genesis (create DB and run server with --genesis)
    genesis(args, shell, &chain_config)
        .await
        .context(MSG_GENESIS_DATABASE_ERR)?;

    Ok(())
}
