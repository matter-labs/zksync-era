use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::parse_abi,
    contract::BaseContract,
    providers::{Http, Middleware, Provider},
    types::Bytes,
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::{
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::GATEWAY_PREPARATION,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::{Address, H256, U256, U64};
use zksync_config::configs::gateway::GatewayChainConfig;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

use crate::{
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
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

lazy_static! {
    static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function migrateChainToGateway(address chainAdmin,address l2ChainAdmin,address accessControlRestriction,uint256 chainId) public",
            "function setDAValidatorPair(address chainAdmin,address accessControlRestriction,uint256 chainId,address l1DAValidator,address l2DAValidator,address chainDiamondProxyOnGateway,address chainAdminOnGateway)",
            "function supplyGatewayWallet(address addr, uint256 addr) public",
            "function enableValidator(address chainAdmin,address accessControlRestriction,uint256 chainId,address validatorAddress,address gatewayValidatorTimelock,address chainAdminOnGateway) public",
            "function grantWhitelist(address filtererProxy, address[] memory addr) public",
            "function deployL2ChainAdmin() public",
            "function notifyServerMigrationFromGateway(address serverNotifier, address chainAdmin, address accessControlRestriction, uint256 chainId) public",
            "function notifyServerMigrationToGateway(address serverNotifier, address chainAdmin, address accessControlRestriction, uint256 chainId) public"
        ])
        .unwrap(),
    );

    static ref BRIDGEHUB_INTERFACE: BaseContract = BaseContract::from(
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
    let gateway_chain_id = gateway_chain_config.chain_id.as_u64();
    let gateway_gateway_config = gateway_chain_config
        .get_gateway_config()
        .context("Gateway config not present")?;

    let l1_url = chain_config
        .get_secrets_config()
        .await?
        .get::<String>("l1.l1_rpc_url")?;

    let genesis_config = chain_config.get_genesis_config().await?;

    let preparation_config_path = GATEWAY_PREPARATION.input(&ecosystem_config.link_to_code);
    let preparation_config = GatewayPreparationConfig::new(
        &gateway_chain_config,
        &gateway_chain_config.get_contracts_config()?,
        &ecosystem_config.get_contracts_config()?,
        &gateway_gateway_config,
    )?;
    preparation_config.save(shell, preparation_config_path)?;

    let chain_contracts_config = chain_config.get_contracts_config().unwrap();
    let chain_admin_addr = chain_contracts_config.l1.chain_admin_addr;
    let chain_access_control_restriction = chain_contracts_config
        .l1
        .access_control_restriction_addr
        .context("chain_access_control_restriction")?;

    logger::info("Whitelisting the chains' addresseses...");
    call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "grantWhitelist",
                (
                    gateway_chain_config
                        .get_contracts_config()?
                        .l1
                        .transaction_filterer_addr
                        .context("transaction_filterer_addr")?,
                    vec![
                        chain_config.get_wallets_config()?.governor.address,
                        chain_config.get_contracts_config()?.l1.chain_admin_addr,
                    ],
                ),
            )
            .unwrap(),
        &chain_config,
        &gateway_chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?;

    logger::info("Migrating the chain to the Gateway...");

    let l2_chain_admin = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode("deployL2ChainAdmin", ())
            .unwrap(),
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?
    .l2_chain_admin_address;
    logger::info(format!(
        "L2 chain admin deployed! Its address: {:#?}",
        l2_chain_admin
    ));

    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "migrateChainToGateway",
                (
                    chain_admin_addr,
                    // TODO(EVM-746): Use L2-based chain admin contract
                    l2_chain_admin,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.as_u64()),
                ),
            )
            .unwrap(),
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;

    let general_config = gateway_chain_config.get_general_config().await?;
    let l2_rpc_url = general_config.get::<String>("api.web3_json_rpc.http_url")?;
    let gateway_provider = Provider::<Http>::try_from(l2_rpc_url.clone())?;

    if hash == H256::zero() {
        logger::info("Chain already migrated!");
    } else {
        logger::info(format!(
            "Migration started! Migration hash: {}",
            hex::encode(hash)
        ));
        await_for_tx_to_complete(&gateway_provider, hash).await?;
    }

    // After the migration is done, there are a few things left to do:
    // Let's grab the new diamond proxy address

    // TODO(EVM-929): maybe move to using a precalculated address, just like for EN
    let chain_id = U256::from(chain_config.chain_id.as_u64());
    let contract = BRIDGEHUB_INTERFACE
        .clone()
        .into_contract(L2_BRIDGEHUB_ADDRESS, gateway_provider);

    let method = contract.method::<U256, Address>("getHyperchain", chain_id)?;

    let new_diamond_proxy_address = method.call().await?;

    logger::info(format!(
        "New diamond proxy address: {}",
        hex::encode(new_diamond_proxy_address.as_bytes())
    ));

    let chain_contracts_config = chain_config.get_contracts_config().unwrap();

    let is_rollup = matches!(
        genesis_config.get("l1_batch_commit_data_generator_mode")?,
        L1BatchCommitmentMode::Rollup
    );

    let gateway_da_validator_address = if is_rollup {
        gateway_gateway_config.relayed_sl_da_validator
    } else {
        gateway_gateway_config.validium_da_validator
    };

    logger::info("Setting DA validator pair...");
    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "setDAValidatorPair",
                (
                    chain_admin_addr,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.as_u64()),
                    gateway_da_validator_address,
                    chain_contracts_config
                        .l2
                        .da_validator_addr
                        .context("da_validator_addr")?,
                    new_diamond_proxy_address,
                    l2_chain_admin,
                ),
            )
            .unwrap(),
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;
    logger::info(format!(
        "DA validator pair set! Hash: {}",
        hex::encode(hash.as_bytes())
    ));

    let chain_secrets_config = chain_config.get_wallets_config().unwrap();

    logger::info("Enabling validators...");
    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "enableValidator",
                (
                    chain_admin_addr,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.as_u64()),
                    chain_secrets_config.blob_operator.address,
                    gateway_gateway_config.validator_timelock_addr,
                    l2_chain_admin,
                ),
            )
            .unwrap(),
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;
    logger::info(format!(
        "blob_operator enabled! Hash: {}",
        hex::encode(hash.as_bytes())
    ));

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
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;
    logger::info(format!(
        "blob_operator supplied with 10 ETH! Hash: {}",
        hex::encode(hash.as_bytes())
    ));

    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "enableValidator",
                (
                    chain_admin_addr,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.as_u64()),
                    chain_secrets_config.operator.address,
                    gateway_gateway_config.validator_timelock_addr,
                    l2_chain_admin,
                ),
            )
            .unwrap(),
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;
    logger::info(format!(
        "operator enabled! Hash: {}",
        hex::encode(hash.as_bytes())
    ));

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
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?
    .governance_l2_tx_hash;
    logger::info(format!(
        "operator supplied with 10 ETH! Hash: {}",
        hex::encode(hash.as_bytes())
    ));

    let gateway_url = l2_rpc_url;
    let mut chain_secrets_config = chain_config.get_secrets_config().await?.patched();
    chain_secrets_config.insert("l1.gateway_rpc_url", gateway_url)?;
    chain_secrets_config.save().await?;

    let gateway_chain_config = GatewayChainConfig::from_gateway_and_chain_data(
        &gateway_gateway_config,
        new_diamond_proxy_address,
        l2_chain_admin,
        gateway_chain_id.into(),
    );
    gateway_chain_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(())
}

async fn await_for_tx_to_complete(
    gateway_provider: &Provider<Http>,
    hash: H256,
) -> anyhow::Result<()> {
    logger::info("Waiting for transaction to complete...");
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
        logger::info("Transaction completed successfully!");
    } else {
        panic!("Transaction failed! Receipt: {:?}", receipt);
    }

    Ok(())
}

pub(crate) enum MigrationDirection {
    FromGateway,
    ToGateway,
}

pub(crate) async fn notify_server(
    args: ForgeScriptArgs,
    shell: &Shell,
    direction: MigrationDirection,
) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let l1_url = chain_config
        .get_secrets_config()
        .await?
        .get::<String>("l1.l1_rpc_url")?;
    let contracts = chain_config.get_contracts_config()?;
    let server_notifier = contracts
        .ecosystem_contracts
        .server_notifier_proxy_addr
        .unwrap();
    let chain_admin = contracts.l1.chain_admin_addr;
    let restrictions = contracts
        .l1
        .access_control_restriction_addr
        .unwrap_or_default();

    let data = match direction {
        MigrationDirection::FromGateway => &GATEWAY_PREPARATION_INTERFACE.encode(
            "notifyServerMigrationFromGateway",
            (
                server_notifier,
                chain_admin,
                restrictions,
                chain_config.chain_id.as_u64(),
            ),
        )?,
        MigrationDirection::ToGateway => &GATEWAY_PREPARATION_INTERFACE.encode(
            "notifyServerMigrationToGateway",
            (
                server_notifier,
                chain_admin,
                restrictions,
                chain_config.chain_id.as_u64(),
            ),
        )?,
    };

    call_script(
        shell,
        args,
        data,
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url,
    )
    .await?;
    Ok(())
}

async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &ChainConfig,
    governor: &Wallet,
    l1_rpc_url: String,
) -> anyhow::Result<GatewayPreparationOutput> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let gateway_preparation_script_output =
        GatewayPreparationOutput::read(shell, GATEWAY_PREPARATION.output(&config.link_to_code))?;

    Ok(gateway_preparation_script_output)
}
