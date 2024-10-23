use std::{fs, fs::File, io::BufReader, path::PathBuf};

use anyhow::Context;
use clap::Parser;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    withdraw::ZKSProvider,
};
use config::{forge_interface::script_params::ZK_PREPARATION, EcosystemConfig};
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
use zksync_basic_types::{H256, U256, U64};
use zksync_types::L2ChainId;
use zksync_web3_decl::client::{Client, L2};

use crate::{
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED},
    utils::forge::{check_the_balance, fill_forge_private_key},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct DeployAndBridgeZKArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub only_funding_tx: bool,
}

#[derive(Debug, Deserialize)]
struct JsonData {
    transactions: Vec<Transaction>,
}

#[derive(Debug, Deserialize)]
struct Transaction {
    hash: String,
}

lazy_static! {
    static ref DEPLOY_AND_BRIDGE_ZK_TOKEN_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function run() public",
            "function supplyEraWallet(address addr, uint256 amount) public",
            "function finalizeZkTokenWithdrawal(uint256 chainId, uint256 l2BatchNumber, uint256 l2MessageIndex, uint16 l2TxNumberInBatch, bytes memory message, bytes32[] memory merkleProof) public",
            "function saveL1Address() public",
            "function fundChainGovernor() public"
        ])
        .unwrap(),
    );
}

fn find_latest_json_file(directory: &str) -> Option<PathBuf> {
    fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map_or(false, |name| name.ends_with("-latest.json"))
        })
}

pub async fn run(args: DeployAndBridgeZKArgs, shell: &Shell) -> anyhow::Result<()> {
    // Setup
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_name = global_config().chain_name.clone();
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

    let era_chain_id = U256::from(chain_config.chain_id.0);
    let era_provider = Provider::<Http>::try_from(
        chain_config
            .get_general_config()
            .unwrap()
            .api_config
            .unwrap()
            .web3_json_rpc
            .http_url,
    )?;
    let era_client: Client<L2> = Client::http(
        chain_config
            .get_general_config()
            .unwrap()
            .api_config
            .unwrap()
            .web3_json_rpc
            .http_url
            .parse()
            .unwrap(),
    )?
    .for_network(L2::from(L2ChainId(chain_config.chain_id.0)))
    .build();

    if args.only_funding_tx {
        let _hash = call_script(
            shell,
            args.forge_args.clone(),
            &DEPLOY_AND_BRIDGE_ZK_TOKEN_INTERFACE
                .encode("fundChainGovernor", ())
                .unwrap(),
            &ecosystem_config,
            chain_config.get_wallets_config()?.governor_private_key(),
            l1_url.clone(),
        )
        .await?;

        return Ok(());
    }

    let _hash = call_script(
        shell,
        args.forge_args.clone(),
        &DEPLOY_AND_BRIDGE_ZK_TOKEN_INTERFACE
            .encode(
                "supplyEraWallet",
                (
                    chain_config.get_wallets_config()?.governor.address,
                    U256::from_dec_str("10000000000000000000").unwrap(),
                ),
            )
            .unwrap(),
        &ecosystem_config,
        chain_config.get_wallets_config()?.governor_private_key(),
        l1_url.clone(),
    )
    .await?;

    println!("Pausing for 20 seconds after funding wallet...");
    tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    println!("Resuming execution");

    // Call the `finalizeZkTokenWithdrawal` function using the extracted parameters
    let calldata = DEPLOY_AND_BRIDGE_ZK_TOKEN_INTERFACE
        .encode("run", ())
        .unwrap();

    let _tx_hash = call_script_era(
        shell,
        args.forge_args.clone(),
        &calldata,
        &ecosystem_config,
        chain_config.get_wallets_config()?.governor_private_key(),
        era_provider.url().to_string(),
    )
    .await?;

    println!("ZK Token Deployed and Withdrawn to L1!");

    println!("Pausing for 20 seconds after withdrawing to L1...");
    tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    println!("Resuming execution");

    let directory = "contracts/l1-contracts/broadcast/DeployZKAndBridgeToL1.s.sol/271/";
    let file_path = find_latest_json_file(directory).ok_or(anyhow::anyhow!(
        "No file with `-latest.json` suffix found in the directory"
    ))?;
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let data: JsonData = serde_json::from_reader(reader)?;
    let tx_hash = if let Some(last_transaction) = data.transactions.last() {
        H256::from_slice(&hex::decode(&last_transaction.hash)?)
    } else {
        anyhow::bail!("No transactions found in the file.");
    };

    // Finalizing transaction
    println!("Withdrawal hash: {}", hex::encode(tx_hash));
    await_for_tx_to_complete(&era_provider, tx_hash).await?;
    await_for_withdrawal_to_finalize(&era_client, tx_hash).await?;

    // Fetch the parameters for calling `finalizeZkTokenWithdrawal`
    let params = era_client
        .get_finalize_withdrawal_params(tx_hash, 0)
        .await?;

    let calldata = DEPLOY_AND_BRIDGE_ZK_TOKEN_INTERFACE
        .encode(
            "finalizeZkTokenWithdrawal",
            (
                era_chain_id,
                U256::from(params.l2_batch_number.0[0]),
                U256::from(params.l2_message_index.0[0]),
                U256::from(params.l2_tx_number_in_block.0[0] as u16),
                params.message,
                params.proof.proof,
            ),
        )
        .unwrap();

    let _tx_hash = call_script(
        shell,
        args.forge_args.clone(),
        &calldata,
        &ecosystem_config,
        chain_config.get_wallets_config()?.governor_private_key(),
        l1_url.clone(),
    )
    .await?;

    println!("ZK Token withdrawal finalization started!");

    let calldata = DEPLOY_AND_BRIDGE_ZK_TOKEN_INTERFACE
        .encode("saveL1Address", ())
        .unwrap();

    let _tx_hash = call_script(
        shell,
        args.forge_args,
        &calldata,
        &ecosystem_config,
        chain_config.get_wallets_config()?.governor_private_key(),
        l1_url,
    )
    .await?;

    println!("ZK Token L1 address saved!");

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
        .script(&ZK_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(rpc_url)
        .with_broadcast()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, private_key)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    // Placeholder for actual output handling
    Ok(H256::zero())
}

async fn call_script_era(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &EcosystemConfig,
    private_key: Option<H256>,
    rpc_url: String,
) -> anyhow::Result<H256> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&ZK_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_zksync()
        .with_rpc_url(rpc_url)
        .with_broadcast()
        .with_slow()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, private_key)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    // Placeholder for actual output handling
    Ok(H256::zero())
}

async fn await_for_tx_to_complete(l2_provider: &Provider<Http>, hash: H256) -> anyhow::Result<()> {
    println!("Waiting for transaction to complete...");
    while Middleware::get_transaction_receipt(l2_provider, hash)
        .await?
        .is_none()
    {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // We do not handle network errors
    let receipt = Middleware::get_transaction_receipt(l2_provider, hash)
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
    l2_provider: &Client<L2>,
    hash: H256,
) -> anyhow::Result<()> {
    println!("Waiting for withdrawal to finalize...");
    while l2_provider.get_withdrawal_log(hash, 0).await.is_err() {
        println!("Waiting for withdrawal to finalize...");
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
    Ok(())
}
