use anyhow::Context;
use ethers::types::Address;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{
    copy_configs, set_l1_rpc_url, traits::SaveConfigWithBasePath, update_from_chain_config,
    ChainConfig, ContractsConfig, EcosystemConfig,
};
use zksync_config::configs::DataAvailabilitySecrets;

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
        MSG_CHAIN_CONFIGS_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR, MSG_CONSENSUS_CONFIG_MISSING_ERR,
        MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR,
    },
    utils::{
        consensus::{generate_consensus_keys, get_consensus_secrets, get_genesis_specs},
        ports::EcosystemPortsScanner,
    },
};

pub async fn run(args: InitConfigsArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
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
    let mut ecosystem_ports = EcosystemPortsScanner::scan(shell)?;
    copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;

    if !init_args.no_port_reallocation {
        ecosystem_ports.allocate_ports_in_yaml(
            shell,
            &chain_config.path_to_general_config(),
            chain_config.id,
        )?;
    }

    let consensus_keys = generate_consensus_keys();

    // Initialize secrets config
    let mut secrets = chain_config.get_secrets_config()?;
    set_l1_rpc_url(&mut secrets, init_args.l1_rpc_url.clone())?;
    secrets.consensus = Some(get_consensus_secrets(&consensus_keys));

    let mut general_config = chain_config.get_general_config()?;

    if general_config.proof_data_handler_config.is_some() && general_config.prover_gateway.is_some()
    {
        let proof_data_handler_config = general_config.proof_data_handler_config.clone().unwrap();
        let mut prover_gateway = general_config.prover_gateway.clone().unwrap();

        prover_gateway.api_url =
            format!("http://127.0.0.1:{}", proof_data_handler_config.http_port);

        general_config.prover_gateway = Some(prover_gateway);
    }

    let mut consensus_config = general_config
        .consensus_config
        .context(MSG_CONSENSUS_CONFIG_MISSING_ERR)?;

    consensus_config.genesis_spec = Some(get_genesis_specs(chain_config, &consensus_keys));

    general_config.consensus_config = Some(consensus_config);
    if let Some(validium_config) = init_args.validium_config.clone() {
        match validium_config {
            ValidiumType::NoDA => {
                general_config.da_client_config = None;
            }
            ValidiumType::Avail((avail_config, avail_secrets)) => {
                general_config.da_client_config = Some(avail_config.into());
                secrets.data_availability = Some(DataAvailabilitySecrets::Avail(avail_secrets));
            }
            ValidiumType::EigenDA => {} // This is left blank to be able to define the config by file instead of giving it via CLI
        }
    }

    secrets.save_with_base_path(shell, &chain_config.configs)?;
    general_config.save_with_base_path(shell, &chain_config.configs)?;

    // Initialize genesis config
    let mut genesis_config = chain_config.get_genesis_config()?;
    update_from_chain_config(&mut genesis_config, chain_config)?;
    genesis_config.save_with_base_path(shell, &chain_config.configs)?;

    // Initialize contracts config
    let mut contracts_config = ecosystem_config.get_contracts_config()?;
    contracts_config.l1.diamond_proxy_addr = Address::zero();
    contracts_config.l1.governance_addr = Address::zero();
    contracts_config.l1.chain_admin_addr = Address::zero();
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    contracts_config.l1.base_token_asset_id = Some(encode_ntv_asset_id(
        genesis_config.l1_chain_id.0.into(),
        contracts_config.l1.base_token_addr,
    ));
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    genesis::database::update_configs(init_args.genesis_args.clone(), shell, chain_config)?;

    update_portal_config(shell, chain_config)
        .await
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    Ok(contracts_config)
}
