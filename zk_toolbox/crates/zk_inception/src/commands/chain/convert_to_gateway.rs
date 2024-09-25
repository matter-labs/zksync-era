use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
};
use config::{
    forge_interface::{
        deploy_ecosystem::input::InitialDeploymentConfig,
        deploy_gateway_ctm::{input::DeployGatewayCTMInput, output::DeployGatewayCTMOutput},
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::{DEPLOY_GATEWAY_CTM, GATEWAY_PREPARATION},
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, GenesisConfig,
};
use ethers::{abi::parse_abi, contract::BaseContract, types::Bytes, utils::hex};
use lazy_static::lazy_static;
use xshell::Shell;
use zksync_basic_types::H256;
use zksync_config::configs::GatewayConfig;

use crate::{
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED},
    utils::forge::{check_the_balance, fill_forge_private_key},
};

lazy_static! {
    static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function governanceRegisterGateway() public",
            "function deployAndSetGatewayTransactionFilterer() public",
            "function governanceWhitelistGatewayCTM(address gatewaySTMAddress, bytes32 governanoceOperationSalt) public",
            "function governanceSetCTMAssetHandler(bytes32 governanoceOperationSalt)",
            "function registerAssetIdInBridgehub(address gatewaySTMAddress, bytes32 governanoceOperationSalt)",
        ])
        .unwrap(),
    );
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let l1_url = chain_config
        .get_secrets_config()?
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();
    let mut chain_contracts_config = chain_config.get_contracts_config()?;
    let chain_genesis_config = chain_config.get_genesis_config()?;

    // Firstly, deploying gateway contracts
    let gateway_config = deploy_gateway_ctm(
        shell,
        args.clone(),
        &ecosystem_config,
        &chain_config,
        &chain_genesis_config,
        &ecosystem_config.get_initial_deployment_config().unwrap(),
        l1_url.clone(),
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

    gateway_governance_whitelisting(
        shell,
        args.clone(),
        &ecosystem_config,
        &chain_config,
        gateway_config,
        l1_url.clone(),
    )
    .await?;

    let output = call_script(
        shell,
        args,
        &GATEWAY_PREPARATION_INTERFACE
            .encode("deployAndSetGatewayTransactionFilterer", ())
            .unwrap(),
        &ecosystem_config,
        &chain_config,
        chain_config.get_wallets_config()?.governor_private_key(),
        l1_url,
    )
    .await?;

    chain_contracts_config.set_transaction_filterer(output.gateway_transaction_filterer_proxy);

    chain_contracts_config.save_with_base_path(shell, chain_config.configs)?;

    Ok(())
}

async fn deploy_gateway_ctm(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    chain_genesis_config: &GenesisConfig,
    initial_deployemnt_config: &InitialDeploymentConfig,
    l1_rpc_url: String,
) -> anyhow::Result<GatewayConfig> {
    let contracts_config = chain_config.get_contracts_config()?;
    let deploy_config_path = DEPLOY_GATEWAY_CTM.input(&config.link_to_code);

    let deploy_config = DeployGatewayCTMInput::new(
        chain_config,
        config,
        chain_genesis_config,
        &contracts_config,
        initial_deployemnt_config,
    );
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&DEPLOY_GATEWAY_CTM.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast();

    // Governor private key should not be needed for this script
    forge = fill_forge_private_key(forge, config.get_wallets()?.deployer_private_key())?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let register_chain_output =
        DeployGatewayCTMOutput::read(shell, DEPLOY_GATEWAY_CTM.output(&chain_config.link_to_code))?;

    let gateway_config: GatewayConfig = register_chain_output.into();

    gateway_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(gateway_config)
}

async fn gateway_governance_whitelisting(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    gateway_config: GatewayConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let hash = call_script(
        shell,
        forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode("governanceRegisterGateway", ())
            .unwrap(),
        config,
        chain_config,
        config.get_wallets()?.governor_private_key(),
        l1_rpc_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;

    println!(
        "Gateway registered as a settlement layer with L2 hash: {}",
        hash
    );

    let hash = call_script(
        shell,
        forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "governanceWhitelistGatewayCTM",
                (gateway_config.state_transition_proxy_addr, H256::random()),
            )
            .unwrap(),
        config,
        chain_config,
        config.get_wallets()?.governor_private_key(),
        l1_rpc_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;

    // Just in case, the L2 tx may or may not fail depending on whether it was executed previously,
    println!(
        "Gateway STM whitelisted L2 hash: {}",
        hex::encode(hash.as_bytes())
    );

    let hash = call_script(
        shell,
        forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode("governanceSetCTMAssetHandler", H256::random())
            .unwrap(),
        config,
        chain_config,
        config.get_wallets()?.governor_private_key(),
        l1_rpc_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;

    // Just in case, the L2 tx may or may not fail depending on whether it was executed previously,
    println!(
        "Gateway STM asset handler is set L2 hash: {}",
        hex::encode(hash.as_bytes())
    );

    let hash = call_script(
        shell,
        forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "registerAssetIdInBridgehub",
                (gateway_config.state_transition_proxy_addr, H256::random()),
            )
            .unwrap(),
        config,
        chain_config,
        config.get_wallets()?.governor_private_key(),
        l1_rpc_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;

    // Just in case, the L2 tx may or may not fail depending on whether it was executed previously,
    println!(
        "Asset Id is registered in L2 bridgehub. L2 hash: {}",
        hex::encode(hash.as_bytes())
    );

    Ok(())
}

async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    private_key: Option<H256>,
    l1_rpc_url: String,
) -> anyhow::Result<GatewayPreparationOutput> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, private_key)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    GatewayPreparationOutput::read(
        shell,
        GATEWAY_PREPARATION.output(&chain_config.link_to_code),
    )
}
