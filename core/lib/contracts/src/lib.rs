//! Set of utility functions to read contracts both in Yul and Sol format.
//!
//! Careful: some of the methods are reading the contracts based on the ZKSYNC_HOME environment variable.

#![allow(clippy::derive_partial_eq_without_eq)]
use ethabi::{
    ethereum_types::{H256, U256},
    Contract,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words};

pub mod test_contracts;

#[derive(Debug, Clone)]
pub enum ContractLanguage {
    Sol,
    Yul,
}

const GOVERNANCE_CONTRACT_FILE: &str =
    "contracts/ethereum/artifacts/cache/solpp-generated-contracts/governance/IGovernance.sol/IGovernance.json";
const ZKSYNC_CONTRACT_FILE: &str =
    "contracts/ethereum/artifacts/cache/solpp-generated-contracts/zksync/interfaces/IZkSync.sol/IZkSync.json";
const MULTICALL3_CONTRACT_FILE: &str =
    "contracts/ethereum/artifacts/cache/solpp-generated-contracts/dev-contracts/Multicall3.sol/Multicall3.json";
const VERIFIER_CONTRACT_FILE: &str =
    "contracts/ethereum/artifacts/cache/solpp-generated-contracts/zksync/Verifier.sol/Verifier.json";
const IERC20_CONTRACT_FILE: &str =
    "contracts/ethereum/artifacts/cache/solpp-generated-contracts/common/interfaces/IERC20.sol/IERC20.json";
const FAIL_ON_RECEIVE_CONTRACT_FILE: &str =
    "contracts/ethereum/artifacts/cache/solpp-generated-contracts/zksync/dev-contracts/FailOnReceive.sol/FailOnReceive.json";
const L2_BRIDGE_CONTRACT_FILE: &str =
    "contracts/zksync/artifacts-zk/cache-zk/solpp-generated-contracts/bridge/interfaces/IL2Bridge.sol/IL2Bridge.json";
const LOADNEXT_CONTRACT_FILE: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/loadnext/loadnext_contract.sol/LoadnextContract.json";
const LOADNEXT_SIMPLE_CONTRACT_FILE: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/loadnext/loadnext_contract.sol/Foo.json";

fn read_file_to_json_value(path: impl AsRef<Path>) -> serde_json::Value {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
    let path = Path::new(&zksync_home).join(path);
    serde_json::from_reader(
        File::open(&path).unwrap_or_else(|e| panic!("Failed to open file {:?}: {}", path, e)),
    )
    .unwrap_or_else(|e| panic!("Failed to parse file {:?}: {}", path, e))
}

pub fn load_contract_if_present<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Option<Contract> {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
    let path = Path::new(&zksync_home).join(path);
    path.exists().then(|| {
        serde_json::from_value(read_file_to_json_value(&path)["abi"].take())
            .unwrap_or_else(|e| panic!("Failed to parse contract abi from file {:?}: {}", path, e))
    })
}

pub fn load_contract<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Contract {
    load_contract_if_present(&path).unwrap_or_else(|| {
        panic!("Failed to load contract from {:?}", path);
    })
}

pub fn load_sys_contract(contract_name: &str) -> Contract {
    load_contract(format!(
        "etc/system-contracts/artifacts-zk/cache-zk/solpp-generated-contracts/{0}.sol/{0}.json",
        contract_name
    ))
}

pub fn read_contract_abi(path: impl AsRef<Path>) -> String {
    read_file_to_json_value(path)["abi"]
        .as_str()
        .expect("Failed to parse abi")
        .to_string()
}

pub fn governance_contract() -> Option<Contract> {
    load_contract_if_present(GOVERNANCE_CONTRACT_FILE)
}

pub fn zksync_contract() -> Contract {
    load_contract(ZKSYNC_CONTRACT_FILE)
}

pub fn multicall_contract() -> Contract {
    load_contract(MULTICALL3_CONTRACT_FILE)
}

pub fn erc20_contract() -> Contract {
    load_contract(IERC20_CONTRACT_FILE)
}

pub fn l2_bridge_contract() -> Contract {
    load_contract(L2_BRIDGE_CONTRACT_FILE)
}

pub fn verifier_contract() -> Contract {
    load_contract(VERIFIER_CONTRACT_FILE)
}

#[derive(Debug, Clone)]
pub struct TestContract {
    /// Contract bytecode to be used for sending deploy transaction.
    pub bytecode: Vec<u8>,
    /// Contract ABI.
    pub contract: Contract,

    pub factory_deps: Vec<Vec<u8>>,
}

/// Reads test contract bytecode and its ABI.
pub fn get_loadnext_contract() -> TestContract {
    let bytecode = read_bytecode(LOADNEXT_CONTRACT_FILE);
    let dep = read_bytecode(LOADNEXT_SIMPLE_CONTRACT_FILE);

    TestContract {
        bytecode,
        contract: loadnext_contract(),
        factory_deps: vec![dep],
    }
}

// Returns loadnext contract and its factory dependencies
pub fn loadnext_contract() -> Contract {
    load_contract("etc/contracts-test-data/artifacts-zk/contracts/loadnext/loadnext_contract.sol/LoadnextContract.json")
}

pub fn loadnext_simple_contract() -> Contract {
    load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/loadnext/loadnext_contract.sol/Foo.json",
    )
}

pub fn fail_on_receive_contract() -> Contract {
    load_contract(FAIL_ON_RECEIVE_CONTRACT_FILE)
}

pub fn deployer_contract() -> Contract {
    load_sys_contract("ContractDeployer")
}

pub fn eth_contract() -> Contract {
    load_sys_contract("L2EthToken")
}

pub fn known_codes_contract() -> Contract {
    load_sys_contract("KnownCodesStorage")
}

/// Reads bytecode from the path RELATIVE to the ZKSYNC_HOME environment variable.
pub fn read_bytecode(relative_path: impl AsRef<Path>) -> Vec<u8> {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
    let artifact_path = Path::new(&zksync_home).join(relative_path);
    read_bytecode_from_path(artifact_path)
}
/// Reads bytecode from a given path.
pub fn read_bytecode_from_path(artifact_path: PathBuf) -> Vec<u8> {
    let artifact = read_file_to_json_value(artifact_path.clone());

    let bytecode = artifact["bytecode"]
        .as_str()
        .unwrap_or_else(|| panic!("Bytecode not found in {:?}", artifact_path))
        .strip_prefix("0x")
        .unwrap_or_else(|| panic!("Bytecode in {:?} is not hex", artifact_path));

    hex::decode(bytecode)
        .unwrap_or_else(|err| panic!("Can't decode bytecode in {:?}: {}", artifact_path, err))
}

pub fn default_erc20_bytecode() -> Vec<u8> {
    read_bytecode("etc/ERC20/artifacts-zk/contracts/ZkSyncERC20.sol/ZkSyncERC20.json")
}

pub fn read_sys_contract_bytecode(directory: &str, name: &str, lang: ContractLanguage) -> Vec<u8> {
    DEFAULT_SYSTEM_CONTRACTS_REPO.read_sys_contract_bytecode(directory, name, lang)
}

pub static DEFAULT_SYSTEM_CONTRACTS_REPO: Lazy<SystemContractsRepo> =
    Lazy::new(SystemContractsRepo::from_env);

/// Structure representing a system contract repository - that allows
/// fetching contracts that are located there.
/// As most of the static methods in this file, is loading data based on ZKSYNC_HOME environment variable.
pub struct SystemContractsRepo {
    // Path to the root of the system contracts repo.
    pub root: PathBuf,
}

impl SystemContractsRepo {
    /// Returns the default system contracts repo with directory based on the ZKSYNC_HOME environment variable.
    pub fn from_env() -> Self {
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let zksync_home = PathBuf::from(zksync_home);
        SystemContractsRepo {
            root: zksync_home.join("etc/system-contracts"),
        }
    }
    pub fn read_sys_contract_bytecode(
        &self,
        directory: &str,
        name: &str,
        lang: ContractLanguage,
    ) -> Vec<u8> {
        match lang {
            ContractLanguage::Sol => read_bytecode_from_path(self.root.join(format!(
                "artifacts-zk/cache-zk/solpp-generated-contracts/{0}{1}.sol/{1}.json",
                directory, name
            ))),
            ContractLanguage::Yul => read_zbin_bytecode_from_path(self.root.join(format!(
                "contracts/{0}artifacts/{1}.yul/{1}.yul.zbin",
                directory, name
            ))),
        }
    }
}

pub fn read_bootloader_code(bootloader_type: &str) -> Vec<u8> {
    read_zbin_bytecode(format!(
        "etc/system-contracts/bootloader/build/artifacts/{}.yul/{}.yul.zbin",
        bootloader_type, bootloader_type
    ))
}

pub fn read_proved_batch_bootloader_bytecode() -> Vec<u8> {
    read_bootloader_code("proved_batch")
}

pub fn read_playground_batch_bootloader_bytecode() -> Vec<u8> {
    read_bootloader_code("playground_batch")
}

pub fn get_loadnext_test_contract_path(file_name: &str, contract_name: &str) -> String {
    format!(
        "core/tests/loadnext/test-contracts/loadnext_contract/artifacts/loadnext_contract.sol/{}.sol:{}.abi",
        file_name, contract_name
    )
}

pub fn get_loadnext_test_contract_bytecode(file_name: &str, contract_name: &str) -> String {
    format!(
        "core/tests/loadnext/test-contracts/loadnext_contract/artifacts/loadnext_contract.sol/{}.sol:{}.zbin",
        file_name, contract_name
    )
}

/// Reads zbin bytecode from a given path, relative to ZKSYNC_HOME.
pub fn read_zbin_bytecode(relative_zbin_path: impl AsRef<Path>) -> Vec<u8> {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
    let bytecode_path = Path::new(&zksync_home).join(relative_zbin_path);
    read_zbin_bytecode_from_path(bytecode_path)
}

/// Reads zbin bytecode from a given path.
pub fn read_zbin_bytecode_from_path(bytecode_path: PathBuf) -> Vec<u8> {
    fs::read(&bytecode_path)
        .unwrap_or_else(|err| panic!("Can't read .zbin bytecode at {:?}: {}", bytecode_path, err))
}
/// Hash of code and code which consists of 32 bytes words
#[derive(Debug, Clone)]
pub struct SystemContractCode {
    pub code: Vec<U256>,
    pub hash: H256,
}

#[derive(Debug, Clone)]
pub struct BaseSystemContracts {
    pub bootloader: SystemContractCode,
    pub default_aa: SystemContractCode,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct BaseSystemContractsHashes {
    pub bootloader: H256,
    pub default_aa: H256,
}

impl PartialEq for BaseSystemContracts {
    fn eq(&self, other: &Self) -> bool {
        self.bootloader.hash == other.bootloader.hash
            && self.default_aa.hash == other.default_aa.hash
    }
}

pub static PLAYGROUND_BLOCK_BOOTLOADER_CODE: Lazy<SystemContractCode> = Lazy::new(|| {
    let bytecode = read_playground_batch_bootloader_bytecode();
    let hash = hash_bytecode(&bytecode);

    SystemContractCode {
        code: bytes_to_be_words(bytecode),
        hash,
    }
});

pub static ESTIMATE_FEE_BLOCK_CODE: Lazy<SystemContractCode> = Lazy::new(|| {
    let bytecode = read_bootloader_code("fee_estimate");
    let hash = hash_bytecode(&bytecode);

    SystemContractCode {
        code: bytes_to_be_words(bytecode),
        hash,
    }
});

impl BaseSystemContracts {
    fn load_with_bootloader(bootloader_bytecode: Vec<u8>) -> Self {
        let hash = hash_bytecode(&bootloader_bytecode);

        let bootloader = SystemContractCode {
            code: bytes_to_be_words(bootloader_bytecode),
            hash,
        };

        let bytecode = read_sys_contract_bytecode("", "DefaultAccount", ContractLanguage::Sol);
        let hash = hash_bytecode(&bytecode);

        let default_aa = SystemContractCode {
            code: bytes_to_be_words(bytecode),
            hash,
        };

        BaseSystemContracts {
            bootloader,
            default_aa,
        }
    }
    // BaseSystemContracts with proved bootloader - for handling transactions.
    pub fn load_from_disk() -> Self {
        let bootloader_bytecode = read_proved_batch_bootloader_bytecode();
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    /// BaseSystemContracts with playground bootloader - used for handling 'eth_calls'.
    pub fn playground() -> Self {
        let bootloader_bytecode = read_playground_batch_bootloader_bytecode();
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    pub fn playground_pre_virtual_blocks() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_3_2/playground_block.yul/playground_block.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    pub fn playground_post_virtual_blocks() -> Self {
        let bootloader_bytecode = read_zbin_bytecode("etc/multivm_bootloaders/vm_virtual_blocks/playground_batch.yul/playground_batch.yul.zbin");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    pub fn playground_post_virtual_blocks_finish_upgrade_fix() -> Self {
        let bootloader_bytecode = read_zbin_bytecode("etc/multivm_bootloaders/vm_virtual_blocks_finish_upgrade_fix/playground_batch.yul/playground_batch.yul.zbin");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    /// BaseSystemContracts with playground bootloader - used for handling 'eth_calls'.
    pub fn estimate_gas() -> Self {
        let bootloader_bytecode = read_bootloader_code("fee_estimate");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    pub fn estimate_gas_pre_virtual_blocks() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_3_2/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    pub fn estimate_gas_post_virtual_blocks() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_virtual_blocks/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    pub fn estimate_gas_post_virtual_blocks_finish_upgrade_fix() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_virtual_blocks_finish_upgrade_fix/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode)
    }

    pub fn hashes(&self) -> BaseSystemContractsHashes {
        BaseSystemContractsHashes {
            bootloader: self.bootloader.hash,
            default_aa: self.default_aa.hash,
        }
    }
}
