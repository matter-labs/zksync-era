use anyhow::{Chain, Context};
use clap::Parser;
use common::{
    config::global_config,
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use config::{
    forge_interface::{
        deploy_ecosystem::input::InitialDeploymentConfig,
        deploy_gateway_ctm::{input::DeployGatewayCTMInput, output::DeployGatewayCTMOutput},
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::{ACCEPT_GOVERNANCE_SCRIPT_PARAMS, DEPLOY_GATEWAY_CTM, GATEWAY_PREPARATION},
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig, GenesisConfig,
};
use ethers::{
    abi::parse_abi,
    contract::BaseContract,
    providers::{Http, Middleware, Provider},
    types::Bytes,
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use types::L1BatchCommitmentMode;
use xshell::Shell;
use zksync_basic_types::{settlement::SettlementMode, Address, H256, U256, U64};
use zksync_config::configs::{
    chain, eth_sender::PubdataSendingMode, gateway::GatewayChainConfig, GatewayConfig,
};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

use crate::{
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED,
        MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO, MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER,
        MSG_WALLETS_CONFIG_MUST_BE_PRESENT, MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct MigrateToGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,
}

// FIXME: use a different script here (i.e. make it have a different file)
lazy_static! {
    static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function migrateChainToGateway(address chainAdmin,address accessControlRestriction,uint256 chainId) public",
            "function setDAValidatorPair(address chainAdmin,address accessControlRestriction,uint256 chainId,address l1DAValidator,address l2DAValidator,address chainDiamondProxyOnGateway)",
            "function supplyGatewayWallet(address addr, uint256 addr) public",
            "function enableValidator(address chainAdmin,address accessControlRestriction,uint256 chainId,address validatorAddress,address gatewayValidatorTimelock) public",
            "function grantWhitelist(address filtererProxy, address[] memory addr) public"
        ])
        .unwrap(),
    );

    static ref BRDIGEHUB_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function getHyperchain(uint256 chainId) public returns (address)"
        ])
        .unwrap(),
    );
}

pub async fn run(args: MigrateToGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let gateway_chain_config = ecosystem_config
        .load_chain(Some(args.gateway_chain_name.clone()))
        .context("Gateway not present")?;
    let gateway_chain_id = gateway_chain_config.chain_id.0;
    let gateway_gateway_config = gateway_chain_config
        .get_gateway_config()
        .context("Gateway config not present")?;

    let l1_url = chain_config
        .get_secrets_config()?
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();

    // FIXME: do we need to build l1 contracts here? they are typically pre-built

    let genesis_config = chain_config.get_genesis_config()?;

    if genesis_config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Validium {
        panic!("Validium is not supported yet!");
    }

    // Firstly, deploying gateway contracts

    let whitelist_config_path = GATEWAY_PREPARATION.input(&ecosystem_config.link_to_code);
    let preparation_config = GatewayPreparationConfig::new(
        &gateway_chain_config,
        &gateway_chain_config.get_contracts_config()?,
        &ecosystem_config.get_contracts_config()?,
        &gateway_gateway_config,
    )?;
    preparation_config.save(shell, whitelist_config_path)?;

    let chain_contracts_config = chain_config
        .get_contracts_config()
        .unwrap();
    let chain_admin_addr = chain_contracts_config
        .l1
        .chain_admin_addr;
    let chain_access_control_restriction = chain_contracts_config.l1.access_control_restriction_addr;

    println!("Whitelisting the chains' addresseses...");
    call_script(
        shell, 
        args.forge_args.clone(), 
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "grantWhitelist",
                (
                    gateway_chain_config.get_contracts_config()?.l1.transaction_filterer_addr,
                    vec![chain_config.get_wallets_config()?.governor.address, chain_config.get_contracts_config()?.l1.chain_admin_addr]
                ),
            )
            .unwrap(), 
        &ecosystem_config, 
        gateway_chain_config
        .get_wallets_config()?
        .governor_private_key(),
        l1_url.clone(),
    )
    .await?;

    println!("Migrating the chain...");

    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "migrateChainToGateway",
                (
                    chain_admin_addr,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.0)
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config
            .get_wallets_config()?
            .governor_private_key(),
        l1_url.clone(),
    )
    .await?;

    let gateway_provider = Provider::<Http>::try_from(
        gateway_chain_config
            .get_general_config()
            .unwrap()
            .api_config
            .unwrap()
            .web3_json_rpc
            .http_url,
    )?;

    if hash == H256::zero() {
        println!("Chain already migrated!");
    } else {
        println!("Migration started! Migration hash: {}", hex::encode(hash));
        await_for_tx_to_complete(&gateway_provider, hash).await?;
    }

    // After the migration is done, there are a few things left to do:
    // Let's grab the new diamond proxy address

    // TODO: maybe move to using a precalculated address, just like for EN
    let chain_id = U256::from(chain_config.chain_id.0);
    let contract = BRDIGEHUB_INTERFACE
        .clone()
        .into_contract(L2_BRIDGEHUB_ADDRESS, gateway_provider);

    let method = contract.method::<U256, Address>("getHyperchain", chain_id)?;

    let new_diamond_proxy_address = method.call().await?;

    println!(
        "New diamond proxy address: {}",
        hex::encode(new_diamond_proxy_address.as_bytes())
    );

    let mut chain_contracts_config = chain_config.get_contracts_config().unwrap();

    println!("Setting DA validator pair...");
    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "setDAValidatorPair",
                (
                    chain_admin_addr,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.0),
                    gateway_gateway_config.relayed_sl_da_validator,
                    chain_contracts_config.l2.l2_da_validator_addr,
                    new_diamond_proxy_address,
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config
        .get_wallets_config()?
        .governor_private_key(),
        l1_url.clone(),
    )
    .await?;
    println!(
        "DA validator pair set! Hash: {}",
        hex::encode(hash.as_bytes())
    );

    let chain_secrets_config = chain_config.get_wallets_config().unwrap();

    println!("Enabling validators...");
    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "enableValidator",
                (
                    chain_admin_addr,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.0),
                    chain_secrets_config.blob_operator.address,
                    gateway_gateway_config.validator_timelock_addr,
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config
        .get_wallets_config()?
        .governor_private_key(),
        l1_url.clone(),
    )
    .await?;
    println!(
        "blob_operator enabled! Hash: {}",
        hex::encode(hash.as_bytes())
    );

    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "supplyGatewayWallet",
                (
                    chain_secrets_config.blob_operator.address,
                    U256::from_dec_str("10000000000000000000").unwrap(),
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config
        .get_wallets_config()?
        .governor_private_key(),
        l1_url.clone(),
    )
    .await?;
    println!(
        "blob_operator supplied with 10 ETH! Hash: {}",
        hex::encode(hash.as_bytes())
    );

    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "enableValidator",
                (
                    chain_admin_addr,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.0),
                    chain_secrets_config.operator.address,
                    gateway_gateway_config.validator_timelock_addr,
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config
        .get_wallets_config()?
        .governor_private_key(),
        l1_url.clone(),
    )
    .await?;
    println!("operator enabled! Hash: {}", hex::encode(hash.as_bytes()));

    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "supplyGatewayWallet",
                (
                    chain_secrets_config.operator.address,
                    U256::from_dec_str("10000000000000000000").unwrap(),
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config
        .get_wallets_config()?
        .governor_private_key(),

        l1_url.clone(),
    )
    .await?;
    println!(
        "operator supplied with 10 ETH! Hash: {}",
        hex::encode(hash.as_bytes())
    );

    // chain_contracts_config.update_after_gateway(
    //     new_diamond_proxy_address,
    //     gateway_gateway_config.validator_timelock_addr,
    //     gateway_gateway_config.multicall3_addr,
    // );
    // chain_contracts_config.save_with_base_path(shell, chain_config.configs.clone())?;

    let gateway_url = gateway_chain_config
        .get_general_config()
        .unwrap()
        .api_config
        .unwrap()
        .web3_json_rpc
        .http_url
        .clone();

    let mut chain_secrets_config = chain_config.get_secrets_config().unwrap();
    chain_secrets_config.l1.as_mut().unwrap().gateway_url =
        Some(url::Url::parse(&gateway_url).unwrap().into());
    chain_secrets_config.save_with_base_path(shell, chain_config.configs.clone())?;

    let gateway_chain_config = GatewayChainConfig::from_gateway_and_chain_data(
        &gateway_gateway_config,
        new_diamond_proxy_address,
        // TODO: for now we do not use a noraml chain admin
        Address::zero(),
    );
    gateway_chain_config.save_with_base_path(shell, chain_config.configs.clone())?;

    let mut general_config = chain_config.get_general_config().unwrap();

    let eth_config = general_config.eth.as_mut().expect("eth");
    let api_config = general_config.api_config.as_mut().expect("api config");

    eth_config
        .gas_adjuster
        .as_mut()
        .expect("gas_adjuster")
        .settlement_mode = SettlementMode::Gateway;
    // FIXME: for now only rollups are supported
    eth_config
        .sender
        .as_mut()
        .expect("sender")
        .pubdata_sending_mode = PubdataSendingMode::RelayedL2Calldata;
    // FIXME: do we need to move the following to be u64?
    eth_config
        .sender
        .as_mut()
        .expect("sender")
        .max_aggregated_tx_gas = 4294967295;
    api_config.web3_json_rpc.settlement_layer_url = Some(gateway_url);
    general_config.save_with_base_path(shell, chain_config.configs.clone())?;

    let mut chain_genesis_config = chain_config.get_genesis_config().unwrap();
    chain_genesis_config.sl_chain_id = Some(gateway_chain_id.into());
    chain_genesis_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(())
}

async fn await_for_tx_to_complete(
    gateway_provider: &Provider<Http>,
    hash: H256,
) -> anyhow::Result<()> {
    println!("Waiting for transaction to complete...");
    while gateway_provider
        .get_transaction_receipt(hash)
        .await?
        .is_none()
    {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // We do not handle network errors
    let receipt = gateway_provider
        .get_transaction_receipt(hash)
        .await?
        .unwrap();

    if receipt.status == Some(U64::from(1)) {
        println!("Transaction completed successfully!");
    } else {
        panic!("Transaction failed!");
    }

    Ok(())
}

async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &EcosystemConfig,
    private_key: Option<H256>,
    l1_rpc_url: String,
) -> anyhow::Result<H256> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(
        forge,
        private_key
    )?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let gateway_preparation_script_output = GatewayPreparationOutput::read(
        shell,
        GATEWAY_PREPARATION.output(&config.link_to_code),
    )?;

    Ok(gateway_preparation_script_output.governance_l2_tx_hash)
}

// pub async fn set_token_multiplier_setter(
//     shell: &Shell,
//     ecosystem_config: &EcosystemConfig,
//     governor: Option<H256>,
//     chain_admin_address: Address,
//     target_address: Address,
//     forge_args: &ForgeScriptArgs,
//     l1_rpc_url: String,
// ) -> anyhow::Result<()> {
//     // Resume for accept admin doesn't work properly. Foundry assumes that if signature of the function is the same,
//     // then it's the same call, but because we are calling this function multiple times during the init process,
//     // code assumes that doing only once is enough, but actually we need to accept admin multiple times
//     let mut forge_args = forge_args.clone();
//     forge_args.resume = false;

//     let calldata = SET_TOKEN_MULTIPLIER_SETTER
//         .encode(
//             "chainSetTokenMultiplierSetter",
//             (chain_admin_address, target_address),
//         )
//         .unwrap();
//     let foundry_contracts_path = ecosystem_config.path_to_l1_foundry();
//     let forge = Forge::new(&foundry_contracts_path)
//         .script(
//             &ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(),
//             forge_args.clone(),
//         )
//         .with_ffi()
//         .with_rpc_url(l1_rpc_url)
//         .with_broadcast()
//         .with_calldata(&calldata);
//     update_token_multiplier_setter(shell, governor, forge).await
// }

// async fn update_token_multiplier_setter(
//     shell: &Shell,
//     governor: Option<H256>,
//     mut forge: ForgeScript,
// ) -> anyhow::Result<()> {
//     forge = fill_forge_private_key(forge, governor)?;
//     check_the_balance(&forge).await?;
//     forge.run(shell)?;
//     Ok(())
// }
