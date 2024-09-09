use common::cmd::Cmd;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use xshell::{cmd, Shell};

use super::args::SendTransactionsArgs;

#[derive(Deserialize)]
struct Transaction {
    from: String,
    gas: String,
    input: String,
    nonce: String,
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

pub fn run(shell: &Shell, args: SendTransactionsArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();
    // Read the JSON file
    let mut file = File::open(args.file).expect("Unable to open file");
    let mut data = String::new();
    file.read_to_string(&mut data).expect("Unable to read file");

    // Parse the JSON file
    let txns: Txns = serde_json::from_str(&data).expect("Unable to parse JSON");

    let private_key = args.private_key;
    let gas_price = args.gas_price;

    // Iterate over each transaction
    for txn in txns.transactions {
        let contract_address = txn.contract_address;
        let from_address = txn.transaction.from;
        let gas = txn.transaction.gas;
        let input_data = txn.transaction.input;
        let nonce = txn.transaction.nonce;

        // Construct the cast send command
        let cmd = Cmd::new(cmd!(shell, "cast send --private-key {private_key} --from {from_address} --gas-limit {gas} --nonce {nonce} --gas-price {gas_price} {contract_address} {input_data}"));

        cmd.run().expect("Failed to execute command")
    }

    Ok(())
}
