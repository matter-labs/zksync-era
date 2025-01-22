use anyhow::Context;
use ethers::{abi::parse_abi, contract::BaseContract, types::Bytes, utils::hex};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::input::InitialDeploymentConfig,
        deploy_gateway_ctm::{input::DeployGatewayCTMInput, output::DeployGatewayCTMOutput},
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::{DEPLOY_GATEWAY_CTM, GATEWAY_GOVERNANCE_TX_PATH1, GATEWAY_PREPARATION},
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, GenesisConfig,
};
use zksync_basic_types::H256;
use zksync_config::configs::GatewayConfig;

use crate::{
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    pub static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function governanceRegisterGateway() public",
            "function deployAndSetGatewayTransactionFilterer() public",
            "function governanceWhitelistGatewayCTM(address gatewaySTMAddress, bytes32 governanoceOperationSalt) public",
            "function governanceSetCTMAssetHandler(bytes32 governanoceOperationSalt)",
            "function registerAssetIdInBridgehub(address gatewaySTMAddress, bytes32 governanoceOperationSalt)",
            "function grantWhitelist(address filtererProxy, address[] memory addr) public",
            "function executeGovernanceTxs() public"
        ])
        .unwrap(),
    );

    static ref DEPLOY_GATEWAY_CTM_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function prepareAddresses() public",
            "function deployCTM() public",
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
    let gateway_config = calculate_gateway_ctm(
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
        true,
    )
    .await?;

    let output = call_script(
        shell,
        args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode("deployAndSetGatewayTransactionFilterer", ())
            .unwrap(),
        &ecosystem_config,
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
        true,
    )
    .await?;

    chain_contracts_config.set_transaction_filterer(output.gateway_transaction_filterer_proxy);

    // We could've deployed the CTM at the beginning however, to be closer to how the actual upgrade
    // looks like we'll do it as the last step

    call_script(
        shell,
        args.clone(),
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
        &ecosystem_config,
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
        true,
    )
    .await?;

    deploy_gateway_ctm(
        shell,
        args,
        &ecosystem_config,
        &chain_config,
        &chain_genesis_config,
        &ecosystem_config.get_initial_deployment_config().unwrap(),
        l1_url,
    )
    .await?;

    chain_contracts_config.save_with_base_path(shell, chain_config.configs)?;

    Ok(())
}

pub async fn calculate_gateway_ctm(
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

    let calldata = DEPLOY_GATEWAY_CTM_INTERFACE
        .encode("prepareAddresses", ())
        .unwrap();

    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&DEPLOY_GATEWAY_CTM.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(&calldata)
        .with_broadcast();

    // Governor private key should not be needed for this script
    forge = fill_forge_private_key(
        forge,
        config.get_wallets()?.deployer.as_ref(),
        WalletOwner::Deployer,
    )?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let register_chain_output =
        DeployGatewayCTMOutput::read(shell, DEPLOY_GATEWAY_CTM.output(&chain_config.link_to_code))?;

    let gateway_config: GatewayConfig = register_chain_output.into();

    gateway_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(gateway_config)
}

pub async fn deploy_gateway_ctm(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    chain_genesis_config: &GenesisConfig,
    initial_deployemnt_config: &InitialDeploymentConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let contracts_config = chain_config.get_contracts_config()?;
    // let contracts_config = config.get_contracts_config()?;
    let deploy_config_path = DEPLOY_GATEWAY_CTM.input(&config.link_to_code);

    let deploy_config = DeployGatewayCTMInput::new(
        chain_config,
        config,
        chain_genesis_config,
        &contracts_config,
        initial_deployemnt_config,
    );
    deploy_config.save(shell, deploy_config_path)?;

    let calldata = DEPLOY_GATEWAY_CTM_INTERFACE
        .encode("deployCTM", ())
        .unwrap();

    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&DEPLOY_GATEWAY_CTM.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(&calldata)
        .with_broadcast();

    // Governor private key should not be needed for this script
    forge = fill_forge_private_key(
        forge,
        config.get_wallets()?.deployer.as_ref(),
        WalletOwner::Deployer,
    )?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    Ok(())
}

pub async fn gateway_governance_whitelisting(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    gateway_config: GatewayConfig,
    l1_rpc_url: String,
    with_broadcast: bool,
) -> anyhow::Result<()> {
    let hash = call_script(
        shell,
        forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode("governanceRegisterGateway", ())
            .unwrap(),
        config,
        chain_config,
        &config.get_wallets()?.governor,
        l1_rpc_url.clone(),
        with_broadcast,
    )
    .await?
    .governance_l2_tx_hash;

    // TOOD(EVM-931): adapt for more generic functionality from the rest of zkstack
    if !with_broadcast {
        // TODO(EVM-930): the path below relies explicitly on chain id 9.
        shell.copy_file(
            config.link_to_code.join("contracts/l1-contracts/broadcast/GatewayPreparation.s.sol/9/dry-run/932d9a4d-latest.json"),
            config.link_to_code.join(GATEWAY_GOVERNANCE_TX_PATH1),
        )?;
    }

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
        &config.get_wallets()?.governor,
        l1_rpc_url.clone(),
        with_broadcast,
    )
    .await?
    .governance_l2_tx_hash;

    // TOOD(EVM-931): adapt for more generic functionality from the rest of zkstack
    if !with_broadcast {
        // TODO(EVM-930): the path below relies explicitly on chain id 9.
        shell.copy_file(
            config.link_to_code.join("contracts/l1-contracts/broadcast/GatewayPreparation.s.sol/9/dry-run/e518d36a-latest.json"),
            config.link_to_code.join(GATEWAY_GOVERNANCE_TX_PATH1),
        )?;
    }

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
        &config.get_wallets()?.governor,
        l1_rpc_url.clone(),
        with_broadcast,
    )
    .await?
    .governance_l2_tx_hash;

    // TOOD(EVM-931): adapt for more generic functionality from the rest of zkstack
    if !with_broadcast {
        // TODO(EVM-930): the path below relies explicitly on chain id 9.
        shell.copy_file(
            config.link_to_code.join("contracts/l1-contracts/broadcast/GatewayPreparation.s.sol/9/dry-run/98b2aab7-latest.json"),
            config.link_to_code.join(GATEWAY_GOVERNANCE_TX_PATH1),
        )?;
    }

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
        &config.get_wallets()?.governor,
        l1_rpc_url.clone(),
        with_broadcast,
    )
    .await?
    .governance_l2_tx_hash;

    // TOOD(EVM-931): adapt for more generic functionality from the rest of zkstack
    if !with_broadcast {
        // TODO(EVM-930): the path below relies explicitly on chain id 9.
        shell.copy_file(
            config.link_to_code.join("contracts/l1-contracts/broadcast/GatewayPreparation.s.sol/9/dry-run/b620eb4c-latest.json"),
            config.link_to_code.join(GATEWAY_GOVERNANCE_TX_PATH1),
        )?;
    }

    // Just in case, the L2 tx may or may not fail depending on whether it was executed previously,
    println!(
        "Asset Id is registered in L2 bridgehub. L2 hash: {}",
        hex::encode(hash.as_bytes())
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    governor: &Wallet,
    l1_rpc_url: String,
    with_broadcast: bool,
) -> anyhow::Result<GatewayPreparationOutput> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(data);
    if with_broadcast {
        forge = forge.with_broadcast();
    }

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    GatewayPreparationOutput::read(
        shell,
        GATEWAY_PREPARATION.output(&chain_config.link_to_code),
    )
}
