use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context};
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    git, logger,
    spinner::Spinner,
};
use config::{
    copy_configs,
    forge_interface::{
        register_chain::{input::RegisterChainL1Config, output::RegisterChainOutput},
        script_params::REGISTER_CHAIN_SCRIPT_PARAMS,
    },
    ports_config, set_l1_rpc_url,
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    update_from_chain_config, update_ports, ChainConfig, ContractsConfig, EcosystemConfig,
    GeneralConfig,
};
use types::{BaseToken, L1Network, WalletCreation};
use xshell::Shell;
use zksync_config::configs::consensus::{
    AttesterPublicKey, AttesterSecretKey, ConsensusConfig, ConsensusSecrets, GenesisSpec, Host,
    NodeSecretKey, ProtocolVersion, Secret, ValidatorPublicKey, ValidatorSecretKey,
    WeightedAttester, WeightedValidator,
};
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles as roles;

use crate::{
    accept_ownership::accept_admin,
    commands::{
        chain::{
            args::init::{InitArgs, InitArgsFinal},
            deploy_l2_contracts, deploy_paymaster,
            genesis::genesis,
            set_token_multiplier_setter::set_token_multiplier_setter,
        },
        portal::update_portal_config,
    },
    consts::{
        AMOUNT_FOR_DISTRIBUTION_TO_WALLETS, GOSSIP_DYNAMIC_INBOUND_LIMIT, MAX_BATCH_SIZE,
        MAX_PAYLOAD_SIZE,
    },
    messages::{
        msg_initializing_chain, MSG_ACCEPTING_ADMIN_SPINNER, MSG_API_CONFIG_MISSING_ERR,
        MSG_CHAIN_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR, MSG_DISTRIBUTING_ETH_SPINNER,
        MSG_GENESIS_DATABASE_ERR, MSG_MINT_BASE_TOKEN_SPINNER,
        MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR, MSG_REGISTERING_CHAIN_SPINNER, MSG_SELECTED_CONFIG,
        MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub(crate) async fn run(args: InitArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let config = EcosystemConfig::from_file(shell)?;
    let chain_config = config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let mut args = args.fill_values_with_prompt(&chain_config);

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));
    logger::info(msg_initializing_chain(""));
    git::submodule_update(shell, config.link_to_code.clone())?;

    init(&mut args, shell, &config, &chain_config).await?;

    logger::success(MSG_CHAIN_INITIALIZED);
    Ok(())
}

pub async fn init(
    init_args: &mut InitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;

    let consensus_keys = generate_consensus_keys();
    let mut general_config = chain_config.get_general_config()?;
    let genesis_spec = Some(get_genesis_specs(chain_config, &consensus_keys));
    let api_config = general_config
        .api_config
        .clone()
        .context(MSG_API_CONFIG_MISSING_ERR)?;
    let public_addr = api_config.web3_json_rpc.http_url.clone();
    let server_addr = public_addr.parse()?;
    let consensus_config = ConsensusConfig {
        server_addr,
        public_addr: Host(public_addr),
        genesis_spec,
        max_payload_size: MAX_PAYLOAD_SIZE,
        gossip_dynamic_inbound_limit: GOSSIP_DYNAMIC_INBOUND_LIMIT,
        max_batch_size: MAX_BATCH_SIZE,
        gossip_static_inbound: BTreeSet::new(),
        gossip_static_outbound: BTreeMap::new(),
        rpc: None,
    };
    apply_port_offset(init_args.port_offset, &mut general_config)?;
    general_config.consensus_config = Some(consensus_config);
    general_config.save_with_base_path(shell, &chain_config.configs)?;

    let mut genesis_config = chain_config.get_genesis_config()?;
    update_from_chain_config(&mut genesis_config, chain_config);
    genesis_config.save_with_base_path(shell, &chain_config.configs)?;

    // Copy ecosystem contracts
    let mut contracts_config = ecosystem_config.get_contracts_config()?;
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    distribute_eth(ecosystem_config, chain_config, init_args.l1_rpc_url.clone()).await?;
    mint_base_token(ecosystem_config, chain_config, init_args.l1_rpc_url.clone()).await?;

    let mut secrets = chain_config.get_secrets_config()?;
    set_l1_rpc_url(&mut secrets, init_args.l1_rpc_url.clone())?;
    secrets.consensus = Some(get_consensus_secrets(&consensus_keys));
    secrets.save_with_base_path(shell, &chain_config.configs)?;

    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    register_chain(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        chain_config,
        &mut contracts_config,
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;
    spinner.finish();
    let spinner = Spinner::new(MSG_ACCEPTING_ADMIN_SPINNER);
    accept_admin(
        shell,
        ecosystem_config,
        contracts_config.l1.chain_admin_addr,
        chain_config.get_wallets_config()?.governor_private_key(),
        contracts_config.l1.diamond_proxy_addr,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    spinner.finish();

    let spinner = Spinner::new(MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER);
    set_token_multiplier_setter(
        shell,
        ecosystem_config,
        chain_config.get_wallets_config()?.governor_private_key(),
        contracts_config.l1.chain_admin_addr,
        ecosystem_config
            .get_wallets()
            .unwrap()
            .token_multiplier_setter
            .address,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    spinner.finish();

    deploy_l2_contracts::deploy_l2_contracts(
        shell,
        chain_config,
        ecosystem_config,
        &mut contracts_config,
        init_args.forge_args.clone(),
    )
    .await?;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    if init_args.deploy_paymaster {
        deploy_paymaster::deploy_paymaster(
            shell,
            chain_config,
            &mut contracts_config,
            init_args.forge_args.clone(),
        )
        .await?;
        contracts_config.save_with_base_path(shell, &chain_config.configs)?;
    }

    genesis(init_args.genesis_args.clone(), shell, chain_config)
        .await
        .context(MSG_GENESIS_DATABASE_ERR)?;

    update_portal_config(shell, chain_config)
        .await
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    Ok(())
}

async fn register_chain(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    contracts: &mut ContractsConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let deploy_config_path = REGISTER_CHAIN_SCRIPT_PARAMS.input(&config.link_to_code);

    let deploy_config = RegisterChainL1Config::new(chain_config, contracts)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&REGISTER_CHAIN_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast();

    forge = fill_forge_private_key(forge, config.get_wallets()?.governor_private_key())?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let register_chain_output = RegisterChainOutput::read(
        shell,
        REGISTER_CHAIN_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;
    contracts.set_chain_contracts(&register_chain_output);
    Ok(())
}

// Distribute eth to the chain wallets for localhost environment
pub async fn distribute_eth(
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    if chain_config.wallet_creation == WalletCreation::Localhost
        && ecosystem_config.l1_network == L1Network::Localhost
    {
        let spinner = Spinner::new(MSG_DISTRIBUTING_ETH_SPINNER);
        let wallets = ecosystem_config.get_wallets()?;
        let chain_wallets = chain_config.get_wallets_config()?;
        let mut addresses = vec![
            chain_wallets.operator.address,
            chain_wallets.blob_operator.address,
            chain_wallets.governor.address,
        ];
        if let Some(deployer) = chain_wallets.deployer {
            addresses.push(deployer.address)
        }
        common::ethereum::distribute_eth(
            wallets.operator,
            addresses,
            l1_rpc_url,
            ecosystem_config.l1_network.chain_id(),
            AMOUNT_FOR_DISTRIBUTION_TO_WALLETS,
        )
        .await?;
        spinner.finish();
    }
    Ok(())
}

pub async fn mint_base_token(
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    if chain_config.wallet_creation == WalletCreation::Localhost
        && ecosystem_config.l1_network == L1Network::Localhost
        && chain_config.base_token != BaseToken::eth()
    {
        let spinner = Spinner::new(MSG_MINT_BASE_TOKEN_SPINNER);
        let wallets = ecosystem_config.get_wallets()?;
        let chain_wallets = chain_config.get_wallets_config()?;
        let base_token = &chain_config.base_token;
        let addresses = vec![wallets.governor.address, chain_wallets.governor.address];
        let amount = AMOUNT_FOR_DISTRIBUTION_TO_WALLETS * base_token.nominator as u128
            / base_token.denominator as u128;
        common::ethereum::mint_token(
            wallets.operator,
            base_token.address,
            addresses,
            l1_rpc_url,
            ecosystem_config.l1_network.chain_id(),
            amount,
        )
        .await?;
        spinner.finish();
    }
    Ok(())
}

fn apply_port_offset(port_offset: u16, general_config: &mut GeneralConfig) -> anyhow::Result<()> {
    let Some(mut ports_config) = ports_config(general_config) else {
        bail!("Missing ports config");
    };

    ports_config.apply_offset(port_offset);

    update_ports(general_config, &ports_config)?;

    Ok(())
}

struct ConsensusKeys {
    validator_key: roles::validator::SecretKey,
    attester_key: roles::attester::SecretKey,
    node_key: roles::node::SecretKey,
}

struct ConsensusPublicKeys {
    validator_key: roles::validator::PublicKey,
    attester_key: roles::attester::PublicKey,
    node_key: roles::node::PublicKey,
}

fn generate_consensus_keys() -> ConsensusKeys {
    ConsensusKeys {
        validator_key: roles::validator::SecretKey::generate(),
        attester_key: roles::attester::SecretKey::generate(),
        node_key: roles::node::SecretKey::generate(),
    }
}

fn get_consensus_public_keys(consensus_keys: &ConsensusKeys) -> ConsensusPublicKeys {
    ConsensusPublicKeys {
        validator_key: consensus_keys.validator_key.public(),
        attester_key: consensus_keys.attester_key.public(),
        node_key: consensus_keys.node_key.public(),
    }
}

fn get_genesis_specs(chain_config: &ChainConfig, consensus_keys: &ConsensusKeys) -> GenesisSpec {
    let public_keys = get_consensus_public_keys(consensus_keys);
    let validator_key = public_keys.validator_key.encode();
    let attester_key = public_keys.attester_key.encode();
    let node_key = public_keys.node_key.encode();

    let validator = WeightedValidator {
        key: ValidatorPublicKey(validator_key),
        weight: 1,
    };
    let attester = WeightedAttester {
        key: AttesterPublicKey(attester_key),
        weight: 1,
    };
    let leader = ValidatorPublicKey(node_key);

    GenesisSpec {
        chain_id: chain_config.chain_id,
        protocol_version: ProtocolVersion(1),
        validators: vec![validator],
        attesters: vec![attester],
        leader,
    }
}

fn get_consensus_secrets(consensus_keys: &ConsensusKeys) -> ConsensusSecrets {
    let validator_key = consensus_keys.validator_key.encode();
    let attester_key = consensus_keys.attester_key.encode();
    let node_key = consensus_keys.node_key.encode();

    ConsensusSecrets {
        validator_key: Some(ValidatorSecretKey(Secret::new(validator_key))),
        attester_key: Some(AttesterSecretKey(Secret::new(attester_key))),
        node_key: Some(NodeSecretKey(Secret::new(node_key))),
    }
}
