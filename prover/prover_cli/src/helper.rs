use std::{
    fs::File,
    path::{Path, PathBuf},
};

use zksync_types::ethabi::Contract;
use zksync_utils::locate_workspace;

const ZKSYNC_HYPERCHAIN_CONTRACT_FILE: &str =
    "contracts/l1-contracts/artifacts/contracts/state-transition/chain-interfaces/IZkSyncHyperchain.sol/IZkSyncHyperchain.json";
const VERIFIER_CONTRACT_FILE: &str =
    "contracts/l1-contracts/artifacts/contracts/state-transition/Verifier.sol/Verifier.json";

pub fn hyperchain_contract() -> Contract {
    read_file_to_json_value(ZKSYNC_HYPERCHAIN_CONTRACT_FILE)
}

pub fn verifier_contract() -> Contract {
    read_file_to_json_value(VERIFIER_CONTRACT_FILE)
}

pub fn read_file_to_json_value(path: &str) -> Contract {
    let home = core_workspace_dir_or_current_dir();
    let path = Path::new(&home).join(path);
    serde_json::from_reader(
        File::open(&path).unwrap_or_else(|e| panic!("Failed to open file {:?}: {}", path, e)),
    )
    .unwrap_or_else(|e| panic!("Failed to parse file {:?}: {}", path, e))
}

pub fn core_workspace_dir_or_current_dir() -> PathBuf {
    locate_workspace()
        .map(|a| a.join(".."))
        .unwrap_or_else(|| PathBuf::from("."))
}
