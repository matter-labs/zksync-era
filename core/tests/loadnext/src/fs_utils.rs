//! Utilities used for reading tokens, contracts bytecode and ABI from the
//! filesystem.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use serde::Deserialize;

use zksync_types::network::Network;
use zksync_types::{ethabi::Contract, Address};

/// A token stored in `etc/tokens/{network}.json` files.
#[derive(Debug, Deserialize)]
pub struct Token {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub address: Address,
}

#[derive(Debug, Clone)]
pub struct TestContract {
    /// Contract bytecode to be used for sending deploy transaction.
    pub bytecode: Vec<u8>,
    /// Contract ABI.
    pub contract: Contract,

    pub factory_deps: Vec<Vec<u8>>,
}

pub fn read_tokens(network: Network) -> anyhow::Result<Vec<Token>> {
    let home = std::env::var("ZKSYNC_HOME")?;
    let path = Path::new(&home);
    let path = path.join(format!("etc/tokens/{network}.json"));

    let file = File::open(path)?;
    let reader = BufReader::new(file);

    Ok(serde_json::from_reader(reader)?)
}

fn extract_bytecode(artifact: &serde_json::Value) -> anyhow::Result<Vec<u8>> {
    let bytecode = artifact["bytecode"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Failed to parse contract bytecode from artifact",))?;

    if let Some(stripped) = bytecode.strip_prefix("0x") {
        hex::decode(stripped)
    } else {
        hex::decode(bytecode)
    }
    .map_err(|e| e.into())
}

/// Reads test contract bytecode and its ABI.
fn read_contract_dir(path: &Path) -> anyhow::Result<TestContract> {
    use serde_json::Value;

    let mut artifact: Value =
        serde_json::from_reader(File::open(path.join("LoadnextContract.json"))?)?;

    let bytecode = extract_bytecode(&artifact)?;

    let abi = artifact["abi"].take();
    let contract: Contract = serde_json::from_value(abi)?;

    let factory_dep: Value = serde_json::from_reader(File::open(path.join("Foo.json"))?)?;
    let factory_dep_bytecode = extract_bytecode(&factory_dep)?;

    anyhow::ensure!(
        contract.functions().count() > 0,
        "Invalid contract: no methods defined: {:?}",
        path
    );
    anyhow::ensure!(
        contract.events().count() > 0,
        "Invalid contract: no events defined: {:?}",
        path
    );

    Ok(TestContract {
        bytecode,
        contract,
        factory_deps: vec![factory_dep_bytecode],
    })
}

pub fn loadnext_contract(path: &Path) -> anyhow::Result<TestContract> {
    let path = path.join("artifacts-zk/contracts/loadnext/loadnext_contract.sol");
    read_contract_dir(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn check_read_test_contract() {
        let test_contracts_path = {
            let home = std::env::var("ZKSYNC_HOME").unwrap();
            let path = PathBuf::from(&home);
            path.join("etc/contracts-test-data")
        };

        loadnext_contract(&test_contracts_path).unwrap();
    }
}
