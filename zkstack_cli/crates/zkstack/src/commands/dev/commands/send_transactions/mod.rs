use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    ops::Add,
    path::Path,
    time::Duration,
};

use anyhow::Context;
use args::SendTransactionsArgs;
use chrono::Local;
use ethers::{abi::Bytes, providers::Middleware, types::TransactionRequest, utils::hex};
use serde::Deserialize;
use tokio::time::sleep;
use xshell::Shell;
use zkstack_cli_common::{ethereum::create_ethers_client, logger};
use zkstack_cli_config::ZkStackConfig;
use zksync_basic_types::{H160, U256};

use crate::commands::dev::{
    consts::DEFAULT_UNSIGNED_TRANSACTIONS_DIR,
    messages::{
        msg_send_txns_outro, MSG_FAILED_TO_SEND_TXN_ERR, MSG_UNABLE_TO_OPEN_FILE_ERR,
        MSG_UNABLE_TO_READ_FILE_ERR, MSG_UNABLE_TO_READ_PARSE_JSON_ERR,
        MSG_UNABLE_TO_WRITE_FILE_ERR,
    },
};

pub mod args;

const MAX_ATTEMPTS: u32 = 3;

#[derive(Deserialize)]
struct Transaction {
    from: String,
    gas: String,
    input: String,
}

#[derive(Deserialize)]
struct Txn {
    #[serde(rename = "contractAddress")]
    contract_address: String,
    transaction: Transaction,
}

#[derive(Deserialize)]
struct Txns {
    transactions: Vec<Txn>,
}

pub async fn run(shell: &Shell, args: SendTransactionsArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();

    let chain_config = ZkStackConfig::current_chain(shell)?;
    let chain_id = chain_config.l1_network.chain_id();

    // Read the JSON file
    let mut file = File::open(args.file).context(MSG_UNABLE_TO_OPEN_FILE_ERR)?;
    let mut data = String::new();
    file.read_to_string(&mut data)
        .context(MSG_UNABLE_TO_READ_FILE_ERR)?;

    // Parse the JSON file
    let txns: Txns = serde_json::from_str(&data).context(MSG_UNABLE_TO_READ_PARSE_JSON_ERR)?;

    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let log_file = chain_config
        .link_to_code
        .join(DEFAULT_UNSIGNED_TRANSACTIONS_DIR)
        .join(format!("{}_receipt.log", timestamp));

    let client = create_ethers_client(args.private_key.parse()?, args.l1_rpc_url, Some(chain_id))?;
    let mut nonce = client.get_transaction_count(client.address(), None).await?;
    let gas_price = client.get_gas_price().await?;

    for txn in txns.transactions {
        let to: H160 = txn.contract_address.parse()?;
        let from: H160 = txn.transaction.from.parse()?;
        let gas_limit: U256 = txn.transaction.gas.parse()?;
        let input_data: Bytes = hex::decode(txn.transaction.input)?;

        let tx = TransactionRequest::new()
            .to(to)
            .from(from)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .data(input_data)
            .chain_id(chain_id);

        nonce = nonce.add(1);

        let mut attempts = 0;
        let receipt = loop {
            attempts += 1;
            match client
                .send_transaction(tx.clone(), None)
                .await?
                .confirmations(args.confirmations)
                .interval(Duration::from_millis(30))
                .await
            {
                Ok(receipt) => break receipt,
                Err(e) if attempts < MAX_ATTEMPTS => {
                    logger::info(format!("Attempt {} failed: {:?}", attempts, e).as_str());
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
                Err(e) => return Err(e).context(MSG_FAILED_TO_SEND_TXN_ERR)?,
            }
        };

        log_receipt(&log_file, format!("{:?}", receipt).as_str())?;
    }

    logger::outro(msg_send_txns_outro(log_file.to_string_lossy().as_ref()));

    Ok(())
}

fn log_receipt(path: &Path, receipt: &str) -> anyhow::Result<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .context(MSG_UNABLE_TO_OPEN_FILE_ERR)?;

    writeln!(file, "{}", receipt).context(MSG_UNABLE_TO_WRITE_FILE_ERR)?;

    Ok(())
}
