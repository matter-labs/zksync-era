use std::{fs::File, path::PathBuf};

use zksync_types::ethabi::Contract;
use zksync_utils::env::Workspace;

const ZKSYNC_HYPERCHAIN_CONTRACT_FILE: &str =
    "contracts/l1-contracts/artifacts/contracts/state-transition/chain-interfaces/IZkSyncHyperchain.sol/IZkSyncHyperchain.json";
const VERIFIER_CONTRACT_FILE: &str =
    "contracts/l1-contracts/artifacts/contracts/state-transition/Verifier.sol/Verifier.json";

pub fn hyperchain_contract() -> Contract {
    load_contract_if_present(ZKSYNC_HYPERCHAIN_CONTRACT_FILE)
}

pub fn verifier_contract() -> Contract {
    load_contract_if_present(VERIFIER_CONTRACT_FILE)
}

fn read_file_to_json_value(path: &PathBuf) -> serde_json::Value {
    serde_json::from_reader(
        File::open(path).unwrap_or_else(|e| panic!("Failed to open file {:?}: {}", path, e)),
    )
    .unwrap_or_else(|e| panic!("Failed to parse file {:?}: {}", path, e))
}

fn load_contract_if_present(path: &str) -> Contract {
    let path = Workspace::locate().root().join(path);
    if path.exists() {
        {
            serde_json::from_value(read_file_to_json_value(&path)["abi"].take()).unwrap_or_else(
                |e| panic!("Failed to parse contract abi from file {:?}: {}", path, e),
            )
        }
    } else {
        {
            panic!("Failed to load contract from {:?}", path);
        }
    }
}
