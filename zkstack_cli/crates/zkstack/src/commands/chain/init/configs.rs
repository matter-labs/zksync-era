use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{
    copy_configs, traits::SaveConfigWithBasePath, ChainConfig, ConsensusGenesisSpecs,
    ContractsConfig, EcosystemConfig, RawConsensusKeys, Weighted, ZkStackConfig,
    ZkStackConfigTrait,
};
use zksync_basic_types::Address;

use crate::{
    commands::{
        chain::{
            args::init::{
                configs::{InitConfigsArgs, InitConfigsArgsFinal},
                da_configs::ValidiumType,
            },
            genesis,
            utils::encode_ntv_asset_id,
        },
        portal::update_portal_config,
    },
    messages::{
        MSG_CHAIN_CONFIGS_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR,
    },
    utils::ports::EcosystemPortsScanner,
};

pub async fn run(args: InitConfigsArgs, shell: &Shell) -> anyhow::Result<()> {
    // TODO Make it possible to run this command without ecosystem config
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let args = args.fill_values_with_prompt(&chain_config);

    init_configs(&args, shell, &ecosystem_config, &chain_config).await?;
    logger::outro(MSG_CHAIN_CONFIGS_INITIALIZED);

    Ok(())
}

pub async fn init_configs(
    init_args: &InitConfigsArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
) -> anyhow::Result<ContractsConfig> {
    // Port scanner should run before copying configs to avoid marking initial ports as assigned
    let mut ecosystem_ports = EcosystemPortsScanner::scan(shell, Some(&chain_config.name))?;
    copy_configs(
        shell,
        &ecosystem_config.default_configs_path(),
        &chain_config.configs,
    )?;

    if !init_args.no_port_reallocation {
        ecosystem_ports.allocate_ports_in_yaml(
            shell,
            &chain_config.path_to_general_config(),
            chain_config.id,
            chain_config.tight_ports,
        )?;
    }

    let general_config = chain_config.get_general_config().await?;
    let prover_data_handler_url = general_config.proof_data_handler_url()?;
    let tee_prover_data_handler_url = general_config.tee_proof_data_handler_url()?;
    let prover_gateway_url = general_config.prover_gateway_url()?;

    let consensus_keys = RawConsensusKeys::generate();

    let mut general_config = general_config.patched();
    if let Some(url) = prover_data_handler_url {
        general_config.set_prover_gateway_url(url)?;
    }
    if let Some(url) = tee_prover_data_handler_url {
        general_config.set_tee_prover_gateway_url(url)?;
    }
    if let Some(url) = prover_gateway_url {
        general_config.set_proof_data_handler_url(url)?;
    }

    general_config.set_consensus_specs(ConsensusGenesisSpecs {
        chain_id: chain_config.chain_id,
        validators: vec![Weighted {
            key: consensus_keys.validator_public.clone(),
            weight: 1,
        }],
        leader: consensus_keys.validator_public.clone(),
    })?;

    match &init_args.validium_config {
        None | Some(ValidiumType::NoDA) | Some(ValidiumType::EigenDA) => {
            general_config.remove_da_client();
        }
        Some(ValidiumType::Avail((avail_config, _))) => {
            general_config.set_avail_client(avail_config)?;
        }
    }
    general_config.save().await?;

    // Initialize genesis config
    let mut genesis_config = chain_config.get_genesis_config().await?.patched();
    genesis_config.update_from_chain_config(chain_config)?;
    genesis_config.save().await?;

    // Initialize contracts config
    let mut contracts_config = ecosystem_config.get_contracts_config()?;
    contracts_config.l1.diamond_proxy_addr = Address::zero();
    contracts_config.l1.governance_addr = Address::zero();
    contracts_config.l1.chain_admin_addr = Address::zero();
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    contracts_config.l1.base_token_asset_id = Some(encode_ntv_asset_id(
        chain_config.l1_network.chain_id().into(),
        contracts_config.l1.base_token_addr,
    ));
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    // Initialize secrets config
    let mut secrets = chain_config.get_secrets_config().await?.patched();
    secrets.set_l1_rpc_url(init_args.l1_rpc_url.clone())?;
    secrets.set_consensus_keys(consensus_keys)?;
    match &init_args.validium_config {
        None | Some(ValidiumType::NoDA) | Some(ValidiumType::EigenDA) => { /* Do nothing */ }
        Some(ValidiumType::Avail((_, avail_secrets))) => {
            secrets.set_avail_secrets(avail_secrets)?;
        }
    }
    secrets.save().await?;

    let override_validium_config = false; // We've initialized validium params above.
    if let Some(genesis_args) = &init_args.genesis_args {
        // Initialize genesis database if needed
        genesis::database::update_configs(
            genesis_args,
            shell,
            chain_config,
            override_validium_config,
        )
        .await?;
    }

    update_portal_config(shell, chain_config)
        .await
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    Ok(contracts_config)
}
