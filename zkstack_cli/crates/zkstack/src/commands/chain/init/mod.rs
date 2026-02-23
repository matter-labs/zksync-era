use anyhow::Context;
use clap::{command, Parser, Subcommand};
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::{
    traits::SaveConfigWithBasePath, ChainConfig, ContractsConfig, CoreContractsConfig,
    EcosystemConfig, ZkStackConfig, ZkStackConfigTrait,
};
use zkstack_cli_types::{BaseToken, L1BatchCommitmentMode, VMOption};
use zksync_basic_types::{commitment::L2DACommitmentScheme, Address};

use crate::{
    admin_functions::{
        accept_admin, make_permanent_rollup, set_da_validator_pair, unpause_deposits,
    },
    commands::chain::{
        args::init::{
            configs::{InitConfigsArgs, InitConfigsArgsFinal},
            da_configs::{ValidiumType, ValidiumTypeInternal},
            InitArgs, InitArgsFinal,
        },
        common::{distribute_eth, mint_base_token},
        deploy_l2_contracts, deploy_paymaster,
        genesis::genesis,
        init::configs::init_configs,
        register_on_all_chains::register_on_all_chains,
        set_token_multiplier_setter::set_token_multiplier_setter,
    },
    enable_evm_emulator::enable_evm_emulator,
    messages::{
        msg_initializing_chain, MSG_CHAIN_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_DA_PAIR_REGISTRATION_SPINNER, MSG_DEPLOYING_PAYMASTER, MSG_GENESIS_DATABASE_ERR,
        MSG_REGISTERING_CHAIN_SPINNER, MSG_SELECTED_CONFIG,
        MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND,
    },
    utils::protocol_ops::{ChainInitProtocolOpsArgs, ProtocolOpsRunner},
};

// Init subcommands
pub mod configs;

#[derive(Subcommand, Debug, Clone)]
pub enum ChainInitSubcommands {
    /// Initialize chain configs
    Configs(InitConfigsArgs),
}

#[derive(Parser, Debug)]
#[command()]
pub struct ChainInitCommand {
    #[command(subcommand)]
    command: Option<ChainInitSubcommands>,
    #[clap(flatten)]
    args: InitArgs,
}

pub(crate) async fn run(args: ChainInitCommand, shell: &Shell) -> anyhow::Result<()> {
    match args.command {
        Some(ChainInitSubcommands::Configs(args)) => configs::run(args, shell).await,
        None => run_init(args.args, shell).await,
    }
}

async fn run_init(args: InitArgs, shell: &Shell) -> anyhow::Result<()> {
    let config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let args = args.fill_values_with_prompt(&chain_config);

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));
    logger::info(msg_initializing_chain(""));

    init(&args, shell, &config, &chain_config).await?;

    logger::success(MSG_CHAIN_INITIALIZED);
    Ok(())
}

pub async fn init(
    init_args: &InitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    // Initialize configs
    let init_configs_args = InitConfigsArgsFinal::from_chain_init_args(init_args);
    init_configs(&init_configs_args, shell, chain_config).await?;

    // Fund some wallet addresses with ETH or base token (only for Localhost)
    distribute_eth(ecosystem_config, chain_config, init_args.l1_rpc_url.clone()).await?;
    mint_base_token(ecosystem_config, chain_config, init_args.l1_rpc_url.clone()).await?;

    let core_contracts = ecosystem_config.get_contracts_config()?;

    // Build protocol_ops chain init args from zkstack configs
    let protocol_ops_args =
        build_chain_init_args(init_args, ecosystem_config, chain_config, &core_contracts)
            .context("Failed to build protocol_ops chain init args")?;

    // Initialize chain using protocol_ops
    let runner = ProtocolOpsRunner::new(ecosystem_config);
    let chain_output = runner.chain_init(shell, &protocol_ops_args)?;

    // Build full ContractsConfig from protocol_ops output + ecosystem contracts
    let contracts_config = chain_output.to_contracts_config(&core_contracts, chain_config);
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    if let Some(genesis_args) = &init_args.genesis_args {
        genesis(genesis_args, shell, chain_config)
            .await
            .context(MSG_GENESIS_DATABASE_ERR)?;
    }

    Ok(())
}

/// Resolve the L1 DA validator address from the ecosystem's core contracts config.
fn resolve_l1_da_validator(
    core_contracts: &CoreContractsConfig,
    chain_config: &ChainConfig,
    validium_config: &Option<ValidiumType>,
) -> anyhow::Result<ethers::types::Address> {
    let ctm = core_contracts.ctm(chain_config.vm_option);

    let addr = match chain_config.l1_batch_commit_data_generator_mode {
        L1BatchCommitmentMode::Rollup => match chain_config.vm_option {
            VMOption::EraVM => Some(ctm.rollup_l1_da_validator_addr),
            VMOption::ZKSyncOsVM => ctm
                .blobs_zksync_os_l1_da_validator_addr
                .or(Some(ctm.rollup_l1_da_validator_addr)),
        },
        L1BatchCommitmentMode::Validium => match validium_config {
            Some(ValidiumType::Avail(_)) => Some(ctm.avail_l1_da_validator_addr),
            Some(ValidiumType::NoDA) | None => Some(ctm.no_da_validium_l1_validator_addr),
            Some(ValidiumType::EigenDA) => Some(ctm.no_da_validium_l1_validator_addr),
        },
    };

    addr.context("L1 DA validator address not found in ecosystem contracts")
}

/// Map chain config to protocol_ops DA mode string.
/// Values must match clap's ValueEnum kebab-case for DAValidatorType variants.
fn resolve_da_mode(chain_config: &ChainConfig, validium_config: &Option<ValidiumType>) -> String {
    match chain_config.l1_batch_commit_data_generator_mode {
        L1BatchCommitmentMode::Rollup => "rollup".to_string(),
        L1BatchCommitmentMode::Validium => match validium_config {
            Some(ValidiumType::Avail(_)) => "avail".to_string(),
            Some(ValidiumType::EigenDA) => "eigen".to_string(),
            Some(ValidiumType::NoDA) | None => "no-da".to_string(),
        },
    }
}

/// Build `ChainInitProtocolOpsArgs` from zkstack configs.
fn build_chain_init_args(
    init_args: &InitArgsFinal,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
    core_contracts: &CoreContractsConfig,
) -> anyhow::Result<ChainInitProtocolOpsArgs> {
    let wallets = chain_config.get_wallets_config()?;
    let ctm = core_contracts.ctm(chain_config.vm_option);

    // Use ecosystem governor's private key as the sender: RegisterZKChain.s.sol
    // needs admin.owner() (= ecosystem governor) for bridgehub admin calls.
    let ecosystem_governor_pk = ecosystem_config
        .get_wallets()?
        .governor
        .private_key_h256()
        .context("Ecosystem governor wallet private key is required")?;

    let l1_da_validator =
        resolve_l1_da_validator(core_contracts, chain_config, &init_args.validium_config)?;

    let da_mode = resolve_da_mode(chain_config, &init_args.validium_config);

    // Chain governor key for acceptOwnership (only needed when chain governor != ecosystem governor)
    let chain_governor_pk = wallets.governor.private_key_h256();

    Ok(ChainInitProtocolOpsArgs {
        private_key: ecosystem_governor_pk,
        owner_private_key: chain_governor_pk,
        l1_rpc_url: init_args.l1_rpc_url.clone(),
        ctm_proxy: ctm.state_transition_proxy_addr,
        l1_da_validator,
        chain_id: chain_config.chain_id.as_u64(),
        base_token_addr: chain_config.base_token.address,
        base_token_price_ratio: format!(
            "{}/{}",
            chain_config.base_token.nominator, chain_config.base_token.denominator
        ),
        da_mode,
        vm_type: chain_config.vm_option,
        owner: wallets.governor.address,
        commit_operator: wallets.blob_operator.address,
        prove_operator: match chain_config.vm_option {
            VMOption::EraVM => wallets.operator.address,
            VMOption::ZKSyncOsVM => wallets.prove_operator.as_ref().map(|w| w.address).unwrap(),
        },
        execute_operator: match chain_config.vm_option {
            VMOption::EraVM => None,
            VMOption::ZKSyncOsVM => wallets.execute_operator.as_ref().map(|w| w.address),
        },
        token_multiplier_setter: wallets.token_multiplier_setter.as_ref().map(|w| w.address),
        governance_addr: wallets.governor.address,
        with_legacy_bridge: chain_config.legacy_bridge.unwrap_or(false),
        pause_deposits: init_args.pause_deposits,
        evm_emulator: chain_config.evm_emulator,
        deploy_paymaster: init_args.deploy_paymaster,
        make_permanent_rollup: init_args.make_permanent_rollup,
        skip_priority_txs: init_args.skip_priority_txs,
        create2_factory_addr: Some(core_contracts.create2_factory_addr),
        create2_factory_salt: Some(core_contracts.create2_factory_salt),
    })
}

// ---- Legacy functions used by gateway migration code ----

pub async fn send_priority_txs(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
    deploy_paymaster: bool,
    validium_type: Option<ValidiumTypeInternal>,
) -> anyhow::Result<()> {
    // Deploy L2 contracts: L2SharedBridge, L2DefaultUpgrader, ... (run by L1 Governor)
    deploy_l2_contracts::deploy_l2_contracts(
        shell,
        chain_config,
        ecosystem_config,
        contracts_config,
        forge_args.clone(),
        true,
        l1_rpc_url.clone(),
    )
    .await?;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    let l1_da_validator_addr = get_l1_da_validator(chain_config, validium_type.clone())
        .await
        .context("l1_da_validator_addr")?;
    let commitment_scheme =
        if chain_config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Rollup {
            match chain_config.vm_option {
                VMOption::EraVM => L2DACommitmentScheme::BlobsAndPubdataKeccak256,
                VMOption::ZKSyncOsVM => L2DACommitmentScheme::BlobsZksyncOS,
            }
        } else {
            // For Validium, use CLI param if provided, otherwise read from general config
            let da_client_type = if let Some(x) = validium_type {
                Some(x.as_str().to_string())
            } else {
                chain_config.get_general_config().await?.da_client_type()
            };

            match da_client_type.as_deref() {
                Some("Avail") | Some("Eigen") => L2DACommitmentScheme::PubdataKeccak256,
                Some("NoDA") | None => L2DACommitmentScheme::EmptyNoDA,
                Some(unsupported) => {
                    anyhow::bail!("DA client config is not supported: {unsupported:?}");
                }
            }
        };

    let spinner = Spinner::new(MSG_DA_PAIR_REGISTRATION_SPINNER);
    set_da_validator_pair(
        shell,
        forge_args,
        &chain_config.path_to_foundry_scripts(),
        crate::admin_functions::AdminScriptMode::Broadcast(
            chain_config.get_wallets_config()?.governor,
        ),
        chain_config.chain_id.as_u64(),
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        l1_da_validator_addr,
        commitment_scheme,
        l1_rpc_url.clone(),
    )
    .await?;
    spinner.finish();

    // Enable EVM emulation if needed (run by L2 Governor)
    if chain_config.evm_emulator {
        enable_evm_emulator(
            shell,
            &chain_config.path_to_foundry_scripts(),
            contracts_config.l1.chain_admin_addr,
            &chain_config.get_wallets_config()?.governor,
            contracts_config.l1.diamond_proxy_addr,
            forge_args,
            l1_rpc_url.clone(),
        )
        .await?;
    }

    // Deploy Paymaster contract (run by L2 Governor)
    if deploy_paymaster {
        let spinner = Spinner::new(MSG_DEPLOYING_PAYMASTER);
        deploy_paymaster::deploy_paymaster(
            shell,
            chain_config,
            contracts_config,
            forge_args.clone(),
            None,
            true,
            l1_rpc_url.clone(),
        )
        .await?;
        contracts_config.save_with_base_path(shell, &chain_config.configs)?;
        spinner.finish();
    }

    // Register chain on all other chains
    register_on_all_chains(
        shell,
        &chain_config.path_to_foundry_scripts(),
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        chain_config.chain_id,
        &chain_config
            .get_wallets_config()?
            .deployer
            .expect("Deployer wallet not set"),
        forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    Ok(())
}

pub(crate) async fn get_l1_da_validator(
    chain_config: &ChainConfig,
    validium_type: Option<ValidiumTypeInternal>,
) -> anyhow::Result<Address> {
    let contracts_config = chain_config.get_contracts_config()?;

    let l1_da_validator_contract = match chain_config.l1_batch_commit_data_generator_mode {
        L1BatchCommitmentMode::Rollup => match chain_config.vm_option {
            VMOption::EraVM => contracts_config.l1.rollup_l1_da_validator_addr,
            VMOption::ZKSyncOsVM => contracts_config.l1.blobs_zksync_os_l1_da_validator_addr,
        },
        L1BatchCommitmentMode::Validium => {
            // Use CLI param if provided, otherwise read from general config
            let da_client_type = if let Some(x) = validium_type {
                Some(x.as_str().to_string())
            } else {
                chain_config.get_general_config().await?.da_client_type()
            };

            match da_client_type.as_deref() {
                Some("Avail") => contracts_config.l1.avail_l1_da_validator_addr,
                Some("NoDA") | None => contracts_config.l1.no_da_validium_l1_validator_addr,
                Some("Eigen") => contracts_config.l1.no_da_validium_l1_validator_addr,
                Some(unsupported) => {
                    anyhow::bail!("DA client config is not supported: {unsupported:?}");
                }
            }
        }
    }
    .context("l1 da validator")?;

    Ok(l1_da_validator_contract)
}
