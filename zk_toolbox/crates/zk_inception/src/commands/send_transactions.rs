use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    ops::Add,
    path::PathBuf,
    time::Duration,
};

use common::ethereum::create_ethers_client;
use config::EcosystemConfig;
use ethers::{abi::Bytes, providers::Middleware, types::TransactionRequest, utils::hex};
use serde::Deserialize;
use xshell::Shell;
use zksync_basic_types::{H160, U256};

use super::args::SendTransactionsArgs;
use crate::consts::DEFAULT_UNSIGNED_TRANSACTIONS_DIR;

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

    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_id = ecosystem_config.l1_network.chain_id();

    // Read the JSON file
    let mut file = File::open(args.file).expect("Unable to open file");
    let mut data = String::new();
    file.read_to_string(&mut data).expect("Unable to read file");

    // Parse the JSON file
    let txns: Txns = serde_json::from_str(&data).expect("Unable to parse JSON");

    let client = create_ethers_client(args.private_key.parse()?, args.l1_rpc_url, Some(chain_id))?;
    let mut nonce = client.get_transaction_count(client.address(), None).await?;

    for txn in txns.transactions {
        let to: H160 = txn.contract_address.parse()?;
        let from: H160 = txn.transaction.from.parse()?;
        let gas_limit: U256 = txn.transaction.gas.parse()?;
        let gas_price: U256 = args.gas_price.parse()?;
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

        let receipt = client
            .send_transaction(tx, None)
            .await?
            .confirmations(args.confirmations)
            .interval(Duration::from_millis(30))
            .await?
            .unwrap();

        log_receipt(
            ecosystem_config
                .link_to_code
                .join(DEFAULT_UNSIGNED_TRANSACTIONS_DIR),
            format!("{:?}", receipt).as_str(),
        );
    }

    Ok(())
}

fn log_receipt(path: PathBuf, receipt: &str) {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path.join("receipt.log"))
        .expect("Unable to open file");

    writeln!(file, "{}", receipt).expect("Unable to write data");
}
