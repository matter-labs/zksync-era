use anyhow::Context;
use clap::Parser;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    withdraw::ZKSProvider,
};
use config::{
    forge_interface::{
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::GATEWAY_PREPARATION,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    EcosystemConfig,
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
use zksync_config::configs::{eth_sender::PubdataSendingMode, gateway::GatewayChainConfig};

use crate::{
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED},
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

// TODO: use a different script here (i.e. make it have a different file)
lazy_static! {
    static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function startMigrateChainFromGateway(address chainAdmin,address accessControlRestriction,uint256 chainId) public",
            "function finishMigrateChainFromGateway(uint256 migratingChainId,uint256 gatewayChainId,uint256 l2BatchNumber,uint256 l2MessageIndex,uint16 l2TxNumberInBatch,bytes memory message,bytes32[] memory merkleProof) public",
            // "function setDAValidatorPair(address chainAdmin,address accessControlRestriction,uint256 chainId,address l1DAValidator,address l2DAValidator,address chainDiamondProxyOnGateway)",
            // "function supplyGatewayWallet(address addr, uint256 addr) public",
            // "function enableValidator(address chainAdmin,address accessControlRestriction,uint256 chainId,address validatorAddress,address gatewayValidatorTimelock) public",
            // "function grantWhitelist(address filtererProxy, address[] memory addr) public"
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

    let genesis_config = chain_config.get_genesis_config()?;

    let is_rollup = matches!(
        genesis_config.l1_batch_commit_data_generator_mode,
        L1BatchCommitmentMode::Rollup
    );

    // Firstly, deploying gateway contracts

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
    let chain_access_control_restriction =
        chain_contracts_config.l1.access_control_restriction_addr;

    println!("Migrating the chain to L1...");
    // println!("gateway_url: {}", gateway_url.clone());
    // let mut forge_args = args.forge_args.clone();
    // forge_args.with_zksync();
    // let hash = H256::from_slice(&hex::decode("18f1005ed5ee9b1a5032808cc304994c29833075e134e8af59cd9e1849972a5c").expect("Invalid hash"));
    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "startMigrateChainFromGateway",
                (
                    chain_admin_addr,
                    chain_access_control_restriction,
                    U256::from(chain_config.chain_id.0),
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config.get_wallets_config()?.governor_private_key(),
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

    // let gateway_provider = ZKSProvider(gateway_provider_pre);

    if hash == H256::zero() {
        println!("Chain already migrated!");
    } else {
        println!("Migration started! Migration hash: {}", hex::encode(hash));
        await_for_tx_to_complete(&gateway_provider, hash).await?;
        await_for_withdrawal_to_finalize(&gateway_provider, hash).await?;
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(20000)).await;

    let params = ZKSProvider::get_finalize_withdrawal_params(&gateway_provider, hash, 0).await?;

    let hash2 = call_script(
        shell,
        args.forge_args,
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "finishMigrateChainFromGateway",
                (
                    U256::from(chain_config.chain_id.0),
                    U256::from(gateway_chain_id),
                    U256::from(params.l2_batch_number.0[0]),
                    U256::from(params.l2_message_index.0[0]),
                    U256::from(params.l2_tx_number_in_block.0[0]),
                    params.message,
                    params.proof.merkle_proof,
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config.get_wallets_config()?.governor_private_key(),
        l1_url.clone(),
    )
    .await?;

    let mut general_config = chain_config.get_general_config().unwrap();

    let eth_config = general_config.eth.as_mut().context("eth")?;
    let api_config = general_config.api_config.as_mut().context("api config")?;

    let gateway_chain_config = GatewayChainConfig::from_gateway_and_chain_data(
        &gateway_gateway_config,
        gateway_chain_config
            .get_contracts_config()?
            .l1
            .diamond_proxy_addr,
        // TODO: for now we do not use a noraml chain admin
        Address::zero(),
        0,
    );
    eth_config
        .gas_adjuster
        .as_mut()
        .expect("gas_adjuster")
        .settlement_mode = SettlementMode::SettlesToL1;
    if is_rollup {
        // For rollups, new type of commitment should be used, but
        // not for validium.
        eth_config
            .sender
            .as_mut()
            .expect("sender")
            .pubdata_sending_mode = PubdataSendingMode::Blobs;
    }
    gateway_chain_config.save_with_base_path(shell, chain_config.configs.clone())?;
    api_config.web3_json_rpc.settlement_layer_url = Some(l1_url);
    general_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(())
}

async fn await_for_tx_to_complete(
    gateway_provider: &Provider<Http>,
    hash: H256,
) -> anyhow::Result<()> {
    println!("Waiting for transaction to complete...");
    while Middleware::get_transaction_receipt(gateway_provider, hash)
        .await?
        .is_none()
    {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // We do not handle network errors
    let receipt = Middleware::get_transaction_receipt(gateway_provider, hash)
        .await?
        .unwrap();

    if receipt.status == Some(U64::from(1)) {
        println!("Transaction completed successfully!");
    } else {
        panic!("Transaction failed!");
    }

    Ok(())
}

async fn await_for_withdrawal_to_finalize(
    gateway_provider: &Provider<Http>,
    hash: H256,
) -> anyhow::Result<()> {
    println!("Waiting for withdrawal to finalize...");
    while ZKSProvider::get_transaction_receipt(gateway_provider, hash)
        .await
        .unwrap_or(None)
        .is_none()
    {
        println!("Waiting for withdrawal to finalize...");
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
    Ok(())
}

async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &EcosystemConfig,
    private_key: Option<H256>,
    rpc_url: String,
) -> anyhow::Result<H256> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(rpc_url)
        .with_broadcast()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, private_key)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let gateway_preparation_script_output =
        GatewayPreparationOutput::read(shell, GATEWAY_PREPARATION.output(&config.link_to_code))?;

    Ok(gateway_preparation_script_output.governance_l2_tx_hash)
}
