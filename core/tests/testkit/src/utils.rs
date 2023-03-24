use anyhow::format_err;
use num::BigUint;
use std::convert::TryFrom;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use zksync_utils::parse_env;

use zksync_contracts::{read_sys_contract_bytecode, ContractLanguage};
use zksync_eth_client::ETHDirectClient;
use zksync_eth_signer::PrivateKeySigner;
use zksync_types::{l1::L1Tx, web3::types::TransactionReceipt, Address};
use zksync_utils::u256_to_biguint;

use crate::types::ETHEREUM_ADDRESS;

pub fn load_test_bytecode_and_calldata() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut dir_path = parse_env::<PathBuf>("ZKSYNC_HOME");
    dir_path.push("etc/contracts-test-data/e");
    let bytecode = read_sys_contract_bytecode("", "Emitter", ContractLanguage::Sol);

    let mut dir_path = parse_env::<PathBuf>("ZKSYNC_HOME");
    dir_path.push("etc");
    dir_path.push("contracts-test-data");
    dir_path.push("events");
    let calldata = {
        let mut calldata_path = dir_path;
        calldata_path.push("sample-calldata");

        let mut calldata_file = File::open(calldata_path).unwrap();
        let mut calldata = Vec::new();
        calldata_file.read_to_end(&mut calldata).unwrap();
        calldata
    };

    (bytecode, Vec::new(), calldata)
}

pub fn l1_tx_from_logs(receipt: &TransactionReceipt) -> L1Tx {
    receipt
        .logs
        .iter()
        .find_map(|op| L1Tx::try_from(op.clone()).ok())
        .expect("failed get L1 tx from logs")
}

pub fn is_token_eth(token_address: Address) -> bool {
    token_address == ETHEREUM_ADDRESS
}

/// Get fee paid in wei for tx execution
pub async fn get_executed_tx_fee(
    client: &ETHDirectClient<PrivateKeySigner>,
    receipt: &TransactionReceipt,
) -> anyhow::Result<BigUint> {
    let gas_used = receipt.gas_used.ok_or_else(|| {
        format_err!(
            "Not used gas in the receipt: 0x{:x?}",
            receipt.transaction_hash
        )
    })?;

    let tx = client
        .get_tx(receipt.transaction_hash, "utils")
        .await?
        .ok_or_else(|| format_err!("Transaction not found: 0x{:x?}", receipt.transaction_hash))?;

    Ok(u256_to_biguint(gas_used * tx.gas_price.unwrap_or_default()))
}
