use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::process::Command;

use super::args::SendTransactionsArgs;

#[derive(Deserialize)]
struct Transaction {
    from: String,
    gas: String,
    value: String,
    input: String,
    nonce: String,
    #[serde(rename = "chainId")]
    chain_id: String,
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

pub fn run(args: SendTransactionsArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();
    // Read the JSON file
    let mut file = File::open(args.file).expect("Unable to open file");
    let mut data = String::new();
    file.read_to_string(&mut data).expect("Unable to read file");

    // Parse the JSON file
    let txns: Txns = serde_json::from_str(&data).expect("Unable to parse JSON");

    // Iterate over each transaction
    for txn in txns.transactions {
        let contract_address = txn.contract_address;
        let from_address = txn.transaction.from;
        let gas = txn.transaction.gas;
        let value = txn.transaction.value;
        let input_data = txn.transaction.input;
        let nonce = txn.transaction.nonce;
        let chain_id = txn.transaction.chain_id;

        // Construct the cast send command
        let output = Command::new("cast")
            .arg("send")
            .arg(contract_address)
            .arg(input_data)
            .arg("--from")
            .arg(from_address)
            .arg("--gas-limit")
            .arg(gas)
            .arg("--value")
            .arg(value)
            .arg("--nonce")
            .arg(nonce)
            .arg("--chain-id")
            .arg(chain_id)
            .output()
            .expect("Failed to execute command");

        // Print the output for debugging
        println!("Output: {:?}", output);
    }

    Ok(())
}
