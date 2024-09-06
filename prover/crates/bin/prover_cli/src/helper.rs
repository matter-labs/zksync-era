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
    let home = core_workspace_dir_or_current_dir();
    let path = Path::new(&home).join(path);
    path.exists()
        .then(|| {
            serde_json::from_value(read_file_to_json_value(&path)["abi"].take()).unwrap_or_else(
                |e| panic!("Failed to parse contract abi from file {:?}: {}", path, e),
            )
        })
        .unwrap_or_else(|| {
            panic!("Failed to load contract from {:?}", path);
        })
}

pub fn core_workspace_dir_or_current_dir() -> PathBuf {
    locate_workspace()
        .map(|a| a.join(".."))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_dir_path() -> anyhow::Result<std::path::PathBuf> {
    let config_dir_path = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find user's config directory"))?
        .join("zks_prover_cli");
    if !config_dir_path.exists() {
        std::fs::create_dir_all(&config_dir_path)?;
    }
    Ok(config_dir_path)
}

pub fn config_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(config_dir_path()?.join(format!("config")))
}
