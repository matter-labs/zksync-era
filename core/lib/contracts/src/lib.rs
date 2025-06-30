//! Set of utility functions to read contracts both in Yul and Sol format.
//!
//! Careful: some of the methods are reading the contracts based on the workspace environment variable.

#![allow(clippy::derive_partial_eq_without_eq)]

use std::{
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{
    bytecode::BytecodeHash,
    ethabi::{Contract, Event, Function},
    H256,
};
use zksync_utils::env::Workspace;

mod serde_bytecode;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub enum ContractLanguage {
    Sol,
    Yul,
}

/// During the transition period we have to support both paths for contracts artifacts
/// One for forge and another for hardhat.
/// Meanwhile, hardhat has one more intermediate folder. That's why, we have to represent each contract
/// by two constants, intermediate folder and actual contract name. For Forge we use only second part
const HARDHAT_PATH_PREFIX: &str = "contracts/l1-contracts/artifacts/contracts";
const FORGE_PATH_PREFIX: &str = "contracts/l1-contracts/out";

const HARDHAT_PROOF_MANAGER_PATH_PREFIX: &str = "proof-manager-contracts/out";
const FORGE_PROOF_MANAGER_PATH_PREFIX: &str = "proof-manager-contracts/out";

const BRIDGEHUB_CONTRACT_FILE: (&str, &str) = ("bridgehub", "IBridgehub.sol/IBridgehub.json");
const STATE_TRANSITION_CONTRACT_FILE: (&str, &str) = (
    "state-transition",
    "ChainTypeManager.sol/ChainTypeManager.json",
);
const BYTECODE_SUPPLIER_CONTRACT_FILE: (&str, &str) =
    ("upgrades", "BytecodesSupplier.sol/BytecodesSupplier.json");
const ZKSYNC_HYPERCHAIN_CONTRACT_FILE: (&str, &str) = (
    "state-transition/chain-interfaces",
    "IZKChain.sol/IZKChain.json",
);
const DIAMOND_INIT_CONTRACT_FILE: (&str, &str) = (
    "state-transition",
    "chain-interfaces/IDiamondInit.sol/IDiamondInit.json",
);
const GOVERNANCE_CONTRACT_FILE: (&str, &str) = ("governance", "IGovernance.sol/IGovernance.json");
// TODO(EVM-924): We currently only support the "Ownable" chain admin.
const CHAIN_ADMIN_CONTRACT_FILE: (&str, &str) = (
    "governance",
    "IChainAdminOwnable.sol/IChainAdminOwnable.json",
);

const SERVER_NOTIFIER_CONTRACT_FILE: (&str, &str) =
    ("governance", "ServerNotifier.sol/ServerNotifier.json");

const GETTERS_FACET_CONTRACT_FILE: (&str, &str) = (
    "state-transition/chain-deps/facets",
    "Getters.sol/GettersFacet.json",
);

const MULTICALL3_CONTRACT_FILE: (&str, &str) = ("dev-contracts", "Multicall3.sol/Multicall3.json");
const L1_ASSET_ROUTER_FILE: (&str, &str) = (
    "bridge/asset-router",
    "L1AssetRouter.sol/L1AssetRouter.json",
);
const L2_WRAPPED_BASE_TOKEN_STORE: (&str, &str) = (
    "bridge",
    "L2WrappedBaseTokenStore.sol/L2WrappedBaseTokenStore.json",
);

const VERIFIER_CONTRACT_FILE: (&str, &str) = ("state-transition", "Verifier.sol/Verifier.json");
const DUAL_VERIFIER_CONTRACT_FILE: (&str, &str) = (
    "state-transition/verifiers",
    "DualVerifier.sol/DualVerifier.json",
);

const PROOF_MANAGER_CONTRACT_FILE: (&str, &str) = ("", "ProofManagerV1.sol/ProofManagerV1.json");

const _IERC20_CONTRACT_FILE: &str =
    "contracts/l1-contracts/artifacts/contracts/common/interfaces/IERC20.sol/IERC20.json";
const _FAIL_ON_RECEIVE_CONTRACT_FILE: &str =
    "contracts/l1-contracts/artifacts/contracts/zksync/dev-contracts/FailOnReceive.sol/FailOnReceive.json";

fn home_path() -> PathBuf {
    Workspace::locate().root()
}

fn read_file_to_json_value(path: impl AsRef<Path> + std::fmt::Debug) -> Option<serde_json::Value> {
    let zksync_home = home_path();
    let path = Path::new(&zksync_home).join(path);
    let file = File::open(&path).ok()?;
    Some(
        serde_json::from_reader(BufReader::new(file))
            .unwrap_or_else(|e| panic!("Failed to parse file {:?}: {}", path, e)),
    )
}

fn load_contract_if_present<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Option<Contract> {
    let zksync_home = home_path();
    let path = Path::new(&zksync_home).join(path);
    path.exists().then(|| {
        serde_json::from_value(read_file_to_json_value(&path).unwrap()["abi"].take())
            .unwrap_or_else(|e| panic!("Failed to parse contract abi from file {:?}: {}", path, e))
    })
}

fn load_contract_for_hardhat(path: (&str, &str)) -> Option<Contract> {
    let path = Path::new(HARDHAT_PATH_PREFIX).join(path.0).join(path.1);
    load_contract_if_present(path)
}

fn load_contract_for_forge(file_path: &str) -> Option<Contract> {
    let path = Path::new(FORGE_PATH_PREFIX).join(file_path);
    load_contract_if_present(path)
}

fn load_contract_for_both_compilers(path: (&str, &str)) -> Contract {
    if let Some(contract) = load_contract_for_forge(path.1) {
        return contract;
    };

    load_contract_for_hardhat(path).unwrap_or_else(|| {
        panic!("Failed to load contract from {:?}", path);
    })
}

fn load_proof_manager_contract(path: (&str, &str)) -> Contract {
    let forge_path = Path::new(FORGE_PROOF_MANAGER_PATH_PREFIX).join(path.1);
    if let Some(contract) = load_contract_if_present(forge_path) {
        return contract;
    };

    let hardhat_path = Path::new(HARDHAT_PROOF_MANAGER_PATH_PREFIX)
        .join(path.0)
        .join(path.1);
    load_contract_if_present(hardhat_path).unwrap_or_else(|| {
        panic!("Failed to load contract from {:?}", path);
    })
}

pub fn load_contract<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Contract {
    load_contract_if_present(&path).unwrap_or_else(|| {
        panic!("Failed to load contract from {:?}", path);
    })
}

pub fn load_sys_contract(contract_name: &str) -> Contract {
    if let Some(contract) = load_contract_if_present(format!(
        "contracts/system-contracts/artifacts-zk/contracts-preprocessed/{0}.sol/{0}.json",
        contract_name
    )) {
        contract
    } else {
        load_contract(format!(
            "contracts/system-contracts/zkout/{0}.sol/{0}.json",
            contract_name
        ))
    }
}

pub fn read_contract_abi(path: impl AsRef<Path> + std::fmt::Debug) -> Option<String> {
    Some(
        read_file_to_json_value(path)?["abi"]
            .as_str()
            .expect("Failed to parse abi")
            .to_string(),
    )
}

pub fn bridgehub_contract() -> Contract {
    load_contract_for_both_compilers(BRIDGEHUB_CONTRACT_FILE)
}

pub fn governance_contract() -> Contract {
    load_contract_for_both_compilers(GOVERNANCE_CONTRACT_FILE)
}

pub fn chain_admin_contract() -> Contract {
    load_contract_for_both_compilers(CHAIN_ADMIN_CONTRACT_FILE)
}

pub fn server_notifier_contract() -> Contract {
    load_contract_for_both_compilers(SERVER_NOTIFIER_CONTRACT_FILE)
}

pub fn getters_facet_contract() -> Contract {
    load_contract_for_both_compilers(GETTERS_FACET_CONTRACT_FILE)
}

pub fn state_transition_manager_contract() -> Contract {
    load_contract_for_both_compilers(STATE_TRANSITION_CONTRACT_FILE)
}

pub fn bytecode_supplier_contract() -> Contract {
    load_contract_for_both_compilers(BYTECODE_SUPPLIER_CONTRACT_FILE)
}

pub fn hyperchain_contract() -> Contract {
    load_contract_for_both_compilers(ZKSYNC_HYPERCHAIN_CONTRACT_FILE)
}

pub fn diamond_init_contract() -> Contract {
    load_contract_for_both_compilers(DIAMOND_INIT_CONTRACT_FILE)
}

pub fn multicall_contract() -> Contract {
    load_contract_for_both_compilers(MULTICALL3_CONTRACT_FILE)
}

pub fn l1_asset_router_contract() -> Contract {
    load_contract_for_both_compilers(L1_ASSET_ROUTER_FILE)
}

pub fn wrapped_base_token_store_contract() -> Contract {
    load_contract_for_both_compilers(L2_WRAPPED_BASE_TOKEN_STORE)
}

pub fn proof_manager_contract() -> Contract {
    load_proof_manager_contract(PROOF_MANAGER_CONTRACT_FILE)
}

pub fn verifier_contract() -> Contract {
    let path = format!("{}/{}", FORGE_PATH_PREFIX, DUAL_VERIFIER_CONTRACT_FILE.1);
    let zksync_home = home_path();
    let path = Path::new(&zksync_home).join(path);

    if path.exists() {
        load_contract_for_both_compilers(DUAL_VERIFIER_CONTRACT_FILE)
    } else {
        load_contract_for_both_compilers(VERIFIER_CONTRACT_FILE)
    }
}

pub fn deployer_contract() -> Contract {
    load_sys_contract("ContractDeployer")
}

pub fn l1_messenger_contract() -> Contract {
    load_sys_contract("L1Messenger")
}

pub fn l2_message_root() -> Contract {
    load_l1_zk_contract("MessageRoot")
}

pub fn l2_asset_router() -> Contract {
    load_l1_zk_contract("L2AssetRouter")
}

pub fn l2_native_token_vault() -> Contract {
    load_l1_zk_contract("L2NativeTokenVault")
}

pub fn l2_legacy_shared_bridge() -> Contract {
    load_l1_zk_contract("L2SharedBridgeLegacy")
}

pub fn l2_rollup_da_validator_bytecode() -> Vec<u8> {
    read_bytecode("contracts/l2-contracts/zkout/RollupL2DAValidator.sol/RollupL2DAValidator.json")
}

pub fn read_l1_zk_contract(name: &str) -> Vec<u8> {
    read_bytecode(format!(
        "contracts/l1-contracts/zkout/{name}.sol/{name}.json"
    ))
}

pub fn load_l1_zk_contract(name: &str) -> Contract {
    load_contract(format!(
        "contracts/l1-contracts/zkout/{name}.sol/{name}.json"
    ))
}

/// Reads bytecode from the path RELATIVE to the Cargo workspace location.
pub fn read_bytecode(relative_path: impl AsRef<Path> + std::fmt::Debug) -> Vec<u8> {
    read_bytecode_from_path(relative_path).expect("Failed to open file")
}

pub fn eth_contract() -> Contract {
    load_sys_contract("L2BaseToken")
}

pub fn known_codes_contract() -> Contract {
    load_sys_contract("KnownCodesStorage")
}

/// Reads bytecode from a given path.
pub fn read_bytecode_from_path(
    artifact_path: impl AsRef<Path> + std::fmt::Debug,
) -> Option<Vec<u8>> {
    let artifact = read_file_to_json_value(&artifact_path)?;

    let bytecode = artifact["bytecode"]
        .as_str()
        .or_else(|| artifact["bytecode"]["object"].as_str())
        .unwrap_or_else(|| panic!("Bytecode not found in {:?}", artifact_path));
    // Strip an optional `0x` prefix.
    let bytecode = bytecode.strip_prefix("0x").unwrap_or(bytecode);
    Some(
        hex::decode(bytecode)
            .unwrap_or_else(|err| panic!("Can't decode bytecode in {:?}: {}", artifact_path, err)),
    )
}

pub fn read_deployed_bytecode_from_path(artifact_path: &Path) -> Option<Vec<u8>> {
    let artifact = read_file_to_json_value(artifact_path)?;
    let bytecode = artifact["deployedBytecode"]
        .as_str()
        .or_else(|| artifact["deployedBytecode"]["object"].as_str())
        .unwrap_or_else(|| panic!("Deployed bytecode not found in {:?}", artifact_path));
    // Strip an optional `0x` prefix.
    let bytecode = bytecode.strip_prefix("0x").unwrap_or(bytecode);
    Some(
        hex::decode(bytecode)
            .unwrap_or_else(|err| panic!("Can't decode bytecode in {:?}: {}", artifact_path, err)),
    )
}

pub fn read_sys_contract_bytecode(directory: &str, name: &str, lang: ContractLanguage) -> Vec<u8> {
    DEFAULT_SYSTEM_CONTRACTS_REPO.read_sys_contract_bytecode(directory, name, None, lang)
}

static DEFAULT_SYSTEM_CONTRACTS_REPO: Lazy<SystemContractsRepo> =
    Lazy::new(SystemContractsRepo::default);

/// Structure representing a system contract repository.
///
/// It allows to fetch contracts that are located there.
/// As most of the static methods in this file, is loading data based on the Cargo workspace location.
pub struct SystemContractsRepo {
    // Path to the root of the system contracts repository.
    pub root: PathBuf,
}

impl Default for SystemContractsRepo {
    /// Returns the default system contracts repository with directory based on the Cargo workspace location.
    fn default() -> Self {
        SystemContractsRepo {
            root: home_path().join("contracts/system-contracts"),
        }
    }
}

impl SystemContractsRepo {
    pub fn read_sys_contract_bytecode(
        &self,
        directory: &str,
        name: &str,
        object_name: Option<&str>,
        lang: ContractLanguage,
    ) -> Vec<u8> {
        match lang {
            ContractLanguage::Sol => {
                let possible_paths = [
                    self.root
                        .join(format!("zkout/{0}{1}.sol/{1}.json", directory, name)),
                    self.root.join(format!(
                        "artifacts-zk/contracts-preprocessed/{0}{1}.sol/{1}.json",
                        directory, name
                    )),
                ];
                for path in &possible_paths {
                    if let Some(contracts) = read_bytecode_from_path(path) {
                        return contracts;
                    }
                }
                panic!("One of the outputs should exist for {directory}{name}. Checked paths: {possible_paths:?}");
            }
            ContractLanguage::Yul => {
                // TODO: Newer versions of foundry-zksync no longer use directory for yul contracts, but we cannot
                // easily get rid of the old lookup, because old foundry-zksync is compiled into `zk_environment`
                // image. Once `foundry-zksync` is updated to at least 0.0.4, we can remove folder names from the
                // `SYSTEM_CONTRACT_LIST` for yul contracts and merge two lookups below.
                // in foundry-zksync starting from 0.0.8 json file name corresponds to the object inside the yul component
                let object_name = object_name.unwrap_or(name);
                let possible_paths = [
                    self.root.join(format!("zkout/{0}.yul/{0}.json", name)),
                    self.root
                        .join(format!("zkout/{0}{1}.yul/{1}.json", directory, name)),
                    self.root.join(format!(
                        "zkout/{name}.yul/contracts-preprocessed/{directory}/{name}.yul.json",
                    )),
                    self.root
                        .join(format!("zkout/{name}.yul/{object_name}.json",)),
                    self.root.join(format!(
                        "zkout/{name}.yul/contracts-preprocessed/{name}.yul.json",
                    )),
                ];

                for path in &possible_paths {
                    if let Some(contracts) = read_bytecode_from_path(path) {
                        return contracts;
                    }
                }

                // Fallback for very old versions.
                let artifacts_path = self
                    .root
                    .join(format!("contracts-preprocessed/{directory}artifacts"));

                let bytecode_path = artifacts_path.join(format!("{name}.yul/{name}.yul.zbin"));
                // Legacy versions of zksolc use the following path for output data if a yul file is being compiled: <name>.yul.zbin
                // New zksolc versions use <name>.yul/<name>.yul.zbin, for consistency with solidity files compilation.
                // In addition, the output of the legacy zksolc in this case is a binary file, while in new versions it is hex encoded.
                if fs::exists(&bytecode_path)
                    .unwrap_or_else(|err| panic!("Invalid path: {bytecode_path:?}, {err}"))
                {
                    read_zbin_bytecode_from_hex_file(bytecode_path)
                } else {
                    let bytecode_path_legacy = artifacts_path.join(format!("{name}.yul.zbin"));

                    if fs::exists(&bytecode_path_legacy).unwrap_or_else(|err| {
                        panic!("Invalid path: {bytecode_path_legacy:?}, {err}")
                    }) {
                        read_zbin_bytecode_from_path(bytecode_path_legacy)
                    } else {
                        panic!(
                            "Can't find bytecode for '{name}' yul contract at {artifacts_path:?} or {possible_paths:?}"
                        )
                    }
                }
            }
        }
    }
}

pub fn read_bootloader_code(bootloader_type: &str) -> Vec<u8> {
    DEFAULT_SYSTEM_CONTRACTS_REPO.read_sys_contract_bytecode(
        "bootloader",
        bootloader_type,
        Some("Bootloader"),
        ContractLanguage::Yul,
    )
}

/// Reads zbin bytecode from a given path, relative to workspace location.
pub fn read_zbin_bytecode(relative_zbin_path: impl AsRef<Path>) -> Vec<u8> {
    let bytecode_path = Path::new(&home_path()).join(relative_zbin_path);
    read_zbin_bytecode_from_path(bytecode_path)
}

/// Reads zbin bytecode from a given path.
fn read_zbin_bytecode_from_path(bytecode_path: PathBuf) -> Vec<u8> {
    fs::read(&bytecode_path)
        .unwrap_or_else(|err| panic!("Can't read .zbin bytecode at {bytecode_path:?}: {err}"))
}

/// Reads zbin bytecode from a given path as utf8 text file.
fn read_zbin_bytecode_from_hex_file(bytecode_path: PathBuf) -> Vec<u8> {
    let bytes = fs::read(&bytecode_path)
        .unwrap_or_else(|err| panic!("Can't read .zbin bytecode at {bytecode_path:?}: {err}"));

    hex::decode(bytes).unwrap_or_else(|err| panic!("Invalid input file: {bytecode_path:?}, {err}"))
}

/// Hash of code and code which consists of 32 bytes words
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContractCode {
    #[serde(with = "serde_bytecode")]
    pub code: Vec<u8>,
    pub hash: H256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseSystemContracts {
    pub bootloader: SystemContractCode,
    pub default_aa: SystemContractCode,
    pub evm_emulator: Option<SystemContractCode>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct BaseSystemContractsHashes {
    pub bootloader: H256,
    pub default_aa: H256,
    /// Optional for backward compatibility reasons. Having a hash present doesn't mean that EVM emulation is enabled for the network.
    pub evm_emulator: Option<H256>,
}

impl PartialEq for BaseSystemContracts {
    fn eq(&self, other: &Self) -> bool {
        self.bootloader.hash == other.bootloader.hash
            && self.default_aa.hash == other.default_aa.hash
            && self.evm_emulator.as_ref().map(|contract| contract.hash)
                == other.evm_emulator.as_ref().map(|contract| contract.hash)
    }
}

impl BaseSystemContracts {
    fn load_with_bootloader(bootloader_bytecode: Vec<u8>, load_evm_emulator: bool) -> Self {
        let hash = BytecodeHash::for_bytecode(&bootloader_bytecode).value();
        let bootloader = SystemContractCode {
            code: bootloader_bytecode,
            hash,
        };

        // `DefaultAccount` is not versioned.
        let bytecode = read_sys_contract_bytecode("", "DefaultAccount", ContractLanguage::Sol);
        let hash = BytecodeHash::for_bytecode(&bytecode).value();
        let default_aa = SystemContractCode {
            code: bytecode,
            hash,
        };

        // EVM emulator is not versioned either. It is only accessed for protocol versions >=27.
        let evm_emulator = load_evm_emulator.then(|| {
            let bytecode = read_sys_contract_bytecode("", "EvmEmulator", ContractLanguage::Yul);
            let hash = BytecodeHash::for_bytecode(&bytecode).value();
            SystemContractCode {
                code: bytecode,
                hash,
            }
        });

        BaseSystemContracts {
            bootloader,
            default_aa,
            evm_emulator,
        }
    }

    /// BaseSystemContracts with proved bootloader - for handling transactions.
    pub fn load_from_disk() -> Self {
        let bootloader_bytecode = read_bootloader_code("proved_batch");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, true)
    }

    /// BaseSystemContracts with playground bootloader - used for handling eth_calls.
    pub fn playground() -> Self {
        let bootloader_bytecode = read_bootloader_code("playground_batch");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, true)
    }

    pub fn playground_pre_virtual_blocks() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_3_2/playground_block.yul/playground_block.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_post_virtual_blocks() -> Self {
        let bootloader_bytecode = read_zbin_bytecode("etc/multivm_bootloaders/vm_virtual_blocks/playground_batch.yul/playground_batch.yul.zbin");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_post_virtual_blocks_finish_upgrade_fix() -> Self {
        let bootloader_bytecode = read_zbin_bytecode("etc/multivm_bootloaders/vm_virtual_blocks_finish_upgrade_fix/playground_batch.yul/playground_batch.yul.zbin");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_post_boojum() -> Self {
        let bootloader_bytecode = read_zbin_bytecode("etc/multivm_bootloaders/vm_boojum_integration/playground_batch.yul/playground_batch.yul.zbin");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_post_allowlist_removal() -> Self {
        let bootloader_bytecode = read_zbin_bytecode("etc/multivm_bootloaders/vm_remove_allowlist/playground_batch.yul/playground_batch.yul.zbin");
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_post_1_4_1() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_4_1/playground_batch.yul/playground_batch.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_post_1_4_2() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_4_2/playground_batch.yul/playground_batch.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_1_5_0_small_memory() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_5_0_small_memory/playground_batch.yul/playground_batch.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_post_1_5_0_increased_memory() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_5_0_increased_memory/playground_batch.yul/playground_batch.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_post_protocol_defense() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_protocol_defense/playground_batch.yul/playground_batch.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_gateway() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_gateway/playground_batch.yul/playground_batch.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn playground_evm_emulator() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
        "etc/multivm_bootloaders/vm_evm_emulator/playground_batch.yul/playground_batch.yul.zbin",
        );

        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, true)
    }

    pub fn playground_precompiles() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_precompiles/playground_batch.yul/Bootloader.zbin",
        );

        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, true)
    }

    pub fn estimate_gas_pre_virtual_blocks() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_3_2/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_post_virtual_blocks() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_virtual_blocks/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_post_virtual_blocks_finish_upgrade_fix() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_virtual_blocks_finish_upgrade_fix/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_post_boojum() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_boojum_integration/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_post_allowlist_removal() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_remove_allowlist/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_post_1_4_1() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_4_1/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_post_1_4_2() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_4_2/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_1_5_0_small_memory() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_5_0_small_memory/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_post_1_5_0_increased_memory() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_1_5_0_increased_memory/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_post_protocol_defense() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_protocol_defense/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_gateway() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_gateway/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, false)
    }

    pub fn estimate_gas_evm_emulator() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_evm_emulator/fee_estimate.yul/fee_estimate.yul.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, true)
    }

    pub fn estimate_gas_precompiles() -> Self {
        let bootloader_bytecode = read_zbin_bytecode(
            "etc/multivm_bootloaders/vm_precompiles/fee_estimate.yul/Bootloader.zbin",
        );
        BaseSystemContracts::load_with_bootloader(bootloader_bytecode, true)
    }

    pub fn hashes(&self) -> BaseSystemContractsHashes {
        BaseSystemContractsHashes {
            bootloader: self.bootloader.hash,
            default_aa: self.default_aa.hash,
            evm_emulator: self.evm_emulator.as_ref().map(|contract| contract.hash),
        }
    }
}

pub static PRE_BOOJUM_COMMIT_FUNCTION: Lazy<Function> = Lazy::new(|| {
    let abi = r#"
    {
      "inputs": [
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "blockNumber",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "blockHash",
              "type": "bytes32"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "l2LogsTreeRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "timestamp",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "commitment",
              "type": "bytes32"
            }
          ],
          "internalType": "struct IExecutor.StoredBlockInfo",
          "name": "_lastCommittedBlockData",
          "type": "tuple"
        },
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "blockNumber",
              "type": "uint64"
            },
            {
              "internalType": "uint64",
              "name": "timestamp",
              "type": "uint64"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "newStateRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "l2LogsTreeRoot",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes",
              "name": "initialStorageChanges",
              "type": "bytes"
            },
            {
              "internalType": "bytes",
              "name": "repeatedStorageChanges",
              "type": "bytes"
            },
            {
              "internalType": "bytes",
              "name": "l2Logs",
              "type": "bytes"
            },
            {
              "internalType": "bytes[]",
              "name": "l2ArbitraryLengthMessages",
              "type": "bytes[]"
            },
            {
              "internalType": "bytes[]",
              "name": "factoryDeps",
              "type": "bytes[]"
            }
          ],
          "internalType": "struct IExecutor.CommitBlockInfo[]",
          "name": "_newBlocksData",
          "type": "tuple[]"
        }
      ],
      "name": "commitBlocks",
      "outputs": [],
      "stateMutability": "nonpayable",
      "type": "function"
    }"#;
    serde_json::from_str(abi).unwrap()
});

pub static GENESIS_UPGRADE_EVENT: Lazy<Event> = Lazy::new(|| {
    let abi = r#"
    {
      "anonymous": false,
      "inputs": [
        {
          "indexed": true,
          "name": "_hyperchain",
          "type": "address"
        },
        {
          "components": [
            {
              "name": "txType",
              "type": "uint256"
            },
            {
              "name": "from",
              "type": "uint256"
            },
            {
              "name": "to",
              "type": "uint256"
            },
            {
              "name": "gasLimit",
              "type": "uint256"
            },
            {
              "name": "gasPerPubdataByteLimit",
              "type": "uint256"
            },
            {
              "name": "maxFeePerGas",
              "type": "uint256"
            },
            {
              "name": "maxPriorityFeePerGas",
              "type": "uint256"
            },
            {
              "name": "paymaster",
              "type": "uint256"
            },
            {
              "name": "nonce",
              "type": "uint256"
            },
            {
              "name": "value",
              "type": "uint256"
            },
            {
              "name": "reserved",
              "type": "uint256[4]"
            },
            {
              "name": "data",
              "type": "bytes"
            },
            {
              "name": "signature",
              "type": "bytes"
            },
            {
              "name": "factoryDeps",
              "type": "uint256[]"
            },
            {
              "name": "paymasterInput",
              "type": "bytes"
            },
            {
              "name": "reservedDynamic",
              "type": "bytes"
            }
          ],
          "indexed": false,
          "name": "_l2Transaction",
          "type": "tuple"
        },
        {
          "indexed": true,
          "name": "_protocolVersion",
          "type": "uint256"
        },
        {
          "indexed": false,
          "name": "_factoryDeps",
          "type": "bytes[]"
        }
      ],
      "name": "GenesisUpgrade",
      "type": "event"
    }"#;
    serde_json::from_str(abi).unwrap()
});

// The function that was used in the pre-v23 versions of the contract to upgrade the diamond proxy.
pub static ADMIN_EXECUTE_UPGRADE_FUNCTION: Lazy<Function> = Lazy::new(|| {
    let abi = r#"
    {
        "inputs": [
          {
            "components": [
              {
                "components": [
                  {
                    "internalType": "address",
                    "name": "facet",
                    "type": "address"
                  },
                  {
                    "internalType": "enum Diamond.Action",
                    "name": "action",
                    "type": "uint8"
                  },
                  {
                    "internalType": "bool",
                    "name": "isFreezable",
                    "type": "bool"
                  },
                  {
                    "internalType": "bytes4[]",
                    "name": "selectors",
                    "type": "bytes4[]"
                  }
                ],
                "internalType": "struct Diamond.FacetCut[]",
                "name": "facetCuts",
                "type": "tuple[]"
              },
              {
                "internalType": "address",
                "name": "initAddress",
                "type": "address"
              },
              {
                "internalType": "bytes",
                "name": "initCalldata",
                "type": "bytes"
              }
            ],
            "internalType": "struct Diamond.DiamondCutData",
            "name": "_diamondCut",
            "type": "tuple"
          }
        ],
        "name": "executeUpgrade",
        "outputs": [],
        "stateMutability": "nonpayable",
        "type": "function"
    }"#;
    serde_json::from_str(abi).unwrap()
});

// The function that is used in post-v23 chains to upgrade the chain
pub static ADMIN_UPGRADE_CHAIN_FROM_VERSION_FUNCTION: Lazy<Function> = Lazy::new(|| {
    let abi = r#"
    {
        "inputs": [
          {
            "internalType": "uint256",
            "name": "_oldProtocolVersion",
            "type": "uint256"
          },
          {
            "components": [
              {
                "components": [
                  {
                    "internalType": "address",
                    "name": "facet",
                    "type": "address"
                  },
                  {
                    "internalType": "enum Diamond.Action",
                    "name": "action",
                    "type": "uint8"
                  },
                  {
                    "internalType": "bool",
                    "name": "isFreezable",
                    "type": "bool"
                  },
                  {
                    "internalType": "bytes4[]",
                    "name": "selectors",
                    "type": "bytes4[]"
                  }
                ],
                "internalType": "struct Diamond.FacetCut[]",
                "name": "facetCuts",
                "type": "tuple[]"
              },
              {
                "internalType": "address",
                "name": "initAddress",
                "type": "address"
              },
              {
                "internalType": "bytes",
                "name": "initCalldata",
                "type": "bytes"
              }
            ],
            "internalType": "struct Diamond.DiamondCutData",
            "name": "_diamondCut",
            "type": "tuple"
          }
        ],
        "name": "upgradeChainFromVersion",
        "outputs": [],
        "stateMutability": "nonpayable",
        "type": "function"
    }"#;
    serde_json::from_str(abi).unwrap()
});

pub static DIAMOND_CUT: Lazy<Function> = Lazy::new(|| {
    let abi = r#"
    {
        "inputs": [
          {
            "components": [
              {
                "components": [
                  {
                    "internalType": "address",
                    "name": "facet",
                    "type": "address"
                  },
                  {
                    "internalType": "enum Diamond.Action",
                    "name": "action",
                    "type": "uint8"
                  },
                  {
                    "internalType": "bool",
                    "name": "isFreezable",
                    "type": "bool"
                  },
                  {
                    "internalType": "bytes4[]",
                    "name": "selectors",
                    "type": "bytes4[]"
                  }
                ],
                "internalType": "struct Diamond.FacetCut[]",
                "name": "facetCuts",
                "type": "tuple[]"
              },
              {
                "internalType": "address",
                "name": "initAddress",
                "type": "address"
              },
              {
                "internalType": "bytes",
                "name": "initCalldata",
                "type": "bytes"
              }
            ],
            "internalType": "struct Diamond.DiamondCutData",
            "name": "_diamondCut",
            "type": "tuple"
          }
        ],
        "name": "diamondCut",
        "outputs": [],
        "stateMutability": "nonpayable",
        "type": "function"
    }"#;
    serde_json::from_str(abi).unwrap()
});

pub static POST_BOOJUM_COMMIT_FUNCTION: Lazy<Function> = Lazy::new(|| {
    let abi = r#"
    {
      "inputs": [
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "batchNumber",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "batchHash",
              "type": "bytes32"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "l2LogsTreeRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "timestamp",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "commitment",
              "type": "bytes32"
            }
          ],
          "internalType": "struct IExecutor.StoredBatchInfo",
          "name": "_lastCommittedBatchData",
          "type": "tuple"
        },
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "batchNumber",
              "type": "uint64"
            },
            {
              "internalType": "uint64",
              "name": "timestamp",
              "type": "uint64"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "newStateRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "bootloaderHeapInitialContentsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "eventsQueueStateHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes",
              "name": "systemLogs",
              "type": "bytes"
            },
            {
              "internalType": "bytes",
              "name": "pubdataCommitments",
              "type": "bytes"
            }
          ],
          "internalType": "struct IExecutor.CommitBatchInfo[]",
          "name": "_newBatchesData",
          "type": "tuple[]"
        }
      ],
      "name": "commitBatches",
      "outputs": [],
      "stateMutability": "nonpayable",
      "type": "function"
    }"#;
    serde_json::from_str(abi).unwrap()
});

pub static POST_SHARED_BRIDGE_COMMIT_FUNCTION: Lazy<Function> = Lazy::new(|| {
    let abi = r#"
    {
      "inputs": [
        {
          "internalType": "uint256",
          "name": "_chainId",
          "type": "uint256"
        },
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "batchNumber",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "batchHash",
              "type": "bytes32"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "l2LogsTreeRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "timestamp",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "commitment",
              "type": "bytes32"
            }
          ],
          "internalType": "struct IExecutor.StoredBatchInfo",
          "name": "_lastCommittedBatchData",
          "type": "tuple"
        },
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "batchNumber",
              "type": "uint64"
            },
            {
              "internalType": "uint64",
              "name": "timestamp",
              "type": "uint64"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "newStateRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "bootloaderHeapInitialContentsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "eventsQueueStateHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes",
              "name": "systemLogs",
              "type": "bytes"
            },
            {
              "internalType": "bytes",
              "name": "pubdataCommitments",
              "type": "bytes"
            }
          ],
          "internalType": "struct IExecutor.CommitBatchInfo[]",
          "name": "_newBatchesData",
          "type": "tuple[]"
        }
      ],
      "name": "commitBatchesSharedBridge",
      "outputs": [],
      "stateMutability": "nonpayable",
      "type": "function"
    }"#;
    serde_json::from_str(abi).unwrap()
});

pub static POST_SHARED_BRIDGE_PROVE_FUNCTION: Lazy<Function> = Lazy::new(|| {
    let abi = r#"
    {
      "inputs": [
        {
          "internalType": "uint256",
          "name": "_chainId",
          "type": "uint256"
        },
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "batchNumber",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "batchHash",
              "type": "bytes32"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "l2LogsTreeRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "timestamp",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "commitment",
              "type": "bytes32"
            }
          ],
          "internalType": "struct IExecutor.StoredBatchInfo",
          "name": "_prevBatch",
          "type": "tuple"
        },
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "batchNumber",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "batchHash",
              "type": "bytes32"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "l2LogsTreeRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "timestamp",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "commitment",
              "type": "bytes32"
            }
          ],
          "internalType": "struct IExecutor.StoredBatchInfo[]",
          "name": "_committedBatches",
          "type": "tuple[]"
        },
        {
          "components": [
            {
              "internalType": "uint256[]",
              "name": "recursiveAggregationInput",
              "type": "uint256[]"
            },
            {
              "internalType": "uint256[]",
              "name": "serializedProof",
              "type": "uint256[]"
            }
          ],
          "internalType": "struct IExecutor.ProofInput",
          "name": "_proof",
          "type": "tuple"
        }
      ],
      "name": "proveBatchesSharedBridge",
      "outputs": [],
      "stateMutability": "nonpayable",
      "type": "function"
    }"#;
    serde_json::from_str(abi).unwrap()
});

pub static POST_SHARED_BRIDGE_EXECUTE_FUNCTION: Lazy<Function> = Lazy::new(|| {
    let abi = r#"
    {
      "inputs": [
        {
          "internalType": "uint256",
          "name": "_chainId",
          "type": "uint256"
        },
        {
          "components": [
            {
              "internalType": "uint64",
              "name": "batchNumber",
              "type": "uint64"
            },
            {
              "internalType": "bytes32",
              "name": "batchHash",
              "type": "bytes32"
            },
            {
              "internalType": "uint64",
              "name": "indexRepeatedStorageChanges",
              "type": "uint64"
            },
            {
              "internalType": "uint256",
              "name": "numberOfLayer1Txs",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "priorityOperationsHash",
              "type": "bytes32"
            },
            {
              "internalType": "bytes32",
              "name": "l2LogsTreeRoot",
              "type": "bytes32"
            },
            {
              "internalType": "uint256",
              "name": "timestamp",
              "type": "uint256"
            },
            {
              "internalType": "bytes32",
              "name": "commitment",
              "type": "bytes32"
            }
          ],
          "internalType": "struct IExecutor.StoredBatchInfo[]",
          "name": "_batchesData",
          "type": "tuple[]"
        }
      ],
      "name": "executeBatchesSharedBridge",
      "outputs": [],
      "stateMutability": "nonpayable",
      "type": "function"
    }"#;
    serde_json::from_str(abi).unwrap()
});
