//! Run external commands from the zk toolkit
//! `zk` script should be in path.

use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;
use zksync_types::{Address, H256};
use zksync_utils::parse_env;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Contracts {
    pub verifier: Address,
    pub zk_sync: Address,
    pub test_erc20_address: Address,
    pub fail_on_receive: Address,
}

fn get_contract_address(deploy_script_out: &str) -> Option<(String, Address)> {
    if let Some(output) = deploy_script_out.strip_prefix("CONTRACTS_PROXY_ADDR=0x") {
        Some((
            String::from("CONTRACTS_PROXY_ADDR"),
            Address::from_str(output).expect("can't parse contract address"),
        ))
    } else if let Some(output) = deploy_script_out.strip_prefix("CONTRACTS_VERIFIER_ADDR=0x") {
        Some((
            String::from("CONTRACTS_VERIFIER_ADDR"),
            Address::from_str(output).expect("can't parse contract address"),
        ))
    } else if let Some(output) = deploy_script_out.strip_prefix("CONTRACTS_TEST_ERC20=0x") {
        Some((
            String::from("CONTRACTS_TEST_ERC20"),
            Address::from_str(output).expect("can't parse contract address"),
        ))
    } else {
        deploy_script_out
            .strip_prefix("CONTRACTS_FAIL_ON_RECEIVE=0x")
            .map(|output| {
                (
                    String::from("CONTRACTS_FAIL_ON_RECEIVE"),
                    Address::from_str(output).expect("can't parse contract address"),
                )
            })
    }
}

/// Runs external command and returns stdout output
fn run_external_command(command: &str, args: &[&str]) -> String {
    let result = Command::new(command)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute command: {}, err: {}", command, e));

    let stdout = String::from_utf8(result.stdout).expect("stdout is not valid utf8");
    let stderr = String::from_utf8(result.stderr).expect("stderr is not valid utf8");

    if !result.status.success() {
        panic!(
            "failed to run external command {}:\nstdout: {}\nstderr: {}",
            command, stdout, stderr
        );
    }
    stdout
}

pub fn deploy_contracts(use_prod_contracts: bool, genesis_root: H256) -> Contracts {
    let mut args = vec!["run", "deploy-testkit"];
    args.push("--genesis-root");
    let genesis_root = format!("0x{:x}", genesis_root);
    args.push(genesis_root.as_str());

    if use_prod_contracts {
        args.push("--prod-contracts");
    }
    let stdout = run_external_command("zk", &args);

    let mut contracts = HashMap::new();
    for std_out_line in stdout.split_whitespace().collect::<Vec<_>>() {
        if let Some((name, address)) = get_contract_address(std_out_line) {
            contracts.insert(name, address);
        }
    }

    Contracts {
        verifier: contracts
            .remove("CONTRACTS_VERIFIER_ADDR")
            .expect("VERIFIER_ADDR missing"),
        zk_sync: contracts
            .remove("CONTRACTS_PROXY_ADDR")
            .expect("CONTRACT_ADDR missing"),
        test_erc20_address: contracts
            .remove("CONTRACTS_TEST_ERC20")
            .expect("TEST_ERC20 missing"),
        fail_on_receive: contracts
            .remove("CONTRACTS_FAIL_ON_RECEIVE")
            .expect("FAIL_ON_RECEIVE missing"),
    }
}

pub fn deploy_erc20_tokens() -> Vec<Address> {
    let args = vec!["run", "deploy-erc20", "dev"];
    run_external_command("zk", &args);

    let mut path = parse_env::<PathBuf>("ZKSYNC_HOME");
    path.push("etc");
    path.push("tokens");
    path.push("localhost.json");
    let text = read_to_string(path).expect("Unable to read file");
    let value: serde_json::Value = serde_json::from_str(&text).expect("Unable to parse");

    let mut addresses = Vec::new();
    for token in value.as_array().expect("Incorrect file format") {
        let address = token["address"].clone().as_str().unwrap().to_string();
        let address = address.strip_prefix("0x").unwrap();
        addresses.push(Address::from_str(address).unwrap());
    }
    addresses
}

pub fn run_upgrade_contract(zksync_address: Address, upgrade_gatekeeper_address: Address) {
    run_external_command(
        "zk",
        &[
            "run",
            "test-upgrade",
            &format!("0x{:x}", zksync_address),
            &format!("0x{:x}", upgrade_gatekeeper_address),
        ],
    );
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EthAccountInfo {
    pub address: Address,
    pub private_key: H256,
}

/// First is vec of test accounts, second is operator account
pub fn get_test_accounts() -> (Vec<EthAccountInfo>, EthAccountInfo) {
    let stdout = run_external_command("zk", &["run", "test-accounts"]);

    let mut parsed = serde_json::from_str::<Vec<EthAccountInfo>>(&stdout)
        .expect("print test accounts script output is not parsed correctly");

    let commit_account = parsed.remove(0);
    assert!(
        !parsed.is_empty(),
        "can't use testkit without test accounts"
    );

    (parsed, commit_account)
}
