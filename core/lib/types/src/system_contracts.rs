use std::path::PathBuf;

use zksync_basic_types::{AccountTreeId, Address, U256};
use zksync_contracts::{read_sys_contract_bytecode, ContractLanguage, SystemContractsRepo};
use zksync_system_constants::{
    BOOTLOADER_UTILITIES_ADDRESS, CODE_ORACLE_ADDRESS, COMPRESSOR_ADDRESS, CREATE2_FACTORY_ADDRESS,
    EVENT_WRITER_ADDRESS, EVM_GAS_MANAGER_ADDRESS, EVM_HASHES_STORAGE_ADDRESS,
    EVM_PREDEPLOYS_MANAGER_ADDRESS, IDENTITY_ADDRESS, L2_ASSET_ROUTER_ADDRESS,
    L2_BRIDGEHUB_ADDRESS, L2_CHAIN_ASSET_HANDLER_ADDRESS, L2_GENESIS_UPGRADE_ADDRESS,
    L2_INTEROP_ROOT_STORAGE_ADDRESS, L2_MESSAGE_ROOT_ADDRESS, L2_MESSAGE_VERIFICATION_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS, L2_WRAPPED_BASE_TOKEN_IMPL, MODEXP_PRECOMPILE_ADDRESS,
    PUBDATA_CHUNK_PUBLISHER_ADDRESS, SECP256R1_VERIFY_PRECOMPILE_ADDRESS, SLOAD_CONTRACT_ADDRESS,
    UPGRADEABLE_BEACON_DEPLOYER_ADDRESS,
};

use crate::{
    block::DeployedContract, ACCOUNT_CODE_STORAGE_ADDRESS, BOOTLOADER_ADDRESS,
    COMPLEX_UPGRADER_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, ECRECOVER_PRECOMPILE_ADDRESS,
    EC_ADD_PRECOMPILE_ADDRESS, EC_MUL_PRECOMPILE_ADDRESS, EC_PAIRING_PRECOMPILE_ADDRESS,
    IMMUTABLE_SIMULATOR_STORAGE_ADDRESS, KECCAK256_PRECOMPILE_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS,
    L1_MESSENGER_ADDRESS, L2_BASE_TOKEN_ADDRESS, MSG_VALUE_SIMULATOR_ADDRESS, NONCE_HOLDER_ADDRESS,
    SHA256_PRECOMPILE_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
};

// Note, that in the `NONCE_HOLDER_ADDRESS` storage the nonces of accounts
// are stored in the following form:
// `2^128 * deployment_nonce + tx_nonce`,
// where `tx_nonce` should be number of transactions, the account has processed
// and the `deployment_nonce` should be the number of contracts.
pub const TX_NONCE_INCREMENT: U256 = U256([1, 0, 0, 0]); // 1
pub const DEPLOYMENT_NONCE_INCREMENT: U256 = U256([0, 0, 1, 0]); // 2^128

static SYSTEM_CONTRACT_LIST: [(&str, &str, Address, ContractLanguage); 41] = [
    (
        "",
        "AccountCodeStorage",
        ACCOUNT_CODE_STORAGE_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "NonceHolder",
        NONCE_HOLDER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "KnownCodesStorage",
        KNOWN_CODES_STORAGE_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "ImmutableSimulator",
        IMMUTABLE_SIMULATOR_STORAGE_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "ContractDeployer",
        CONTRACT_DEPLOYER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "L1Messenger",
        L1_MESSENGER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "MsgValueSimulator",
        MSG_VALUE_SIMULATOR_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "L2BaseToken",
        L2_BASE_TOKEN_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "precompiles/",
        "Keccak256",
        KECCAK256_PRECOMPILE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "SHA256",
        SHA256_PRECOMPILE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "Ecrecover",
        ECRECOVER_PRECOMPILE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "Modexp",
        MODEXP_PRECOMPILE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "EcAdd",
        EC_ADD_PRECOMPILE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "EcMul",
        EC_MUL_PRECOMPILE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "EcPairing",
        EC_PAIRING_PRECOMPILE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "P256Verify",
        SECP256R1_VERIFY_PRECOMPILE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "CodeOracle",
        CODE_ORACLE_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "precompiles/",
        "Identity",
        IDENTITY_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "",
        "SystemContext",
        SYSTEM_CONTEXT_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "EventWriter",
        EVENT_WRITER_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "",
        "BootloaderUtilities",
        BOOTLOADER_UTILITIES_ADDRESS,
        ContractLanguage::Sol,
    ),
    ("", "Compressor", COMPRESSOR_ADDRESS, ContractLanguage::Sol),
    (
        "../../l1-contracts/zkout/",
        "L2ComplexUpgrader",
        COMPLEX_UPGRADER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "EvmGasManager",
        EVM_GAS_MANAGER_ADDRESS,
        ContractLanguage::Yul,
    ),
    (
        "",
        "EvmPredeploysManager",
        EVM_PREDEPLOYS_MANAGER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "EvmHashesStorage",
        EVM_HASHES_STORAGE_ADDRESS,
        ContractLanguage::Sol,
    ),
    // For now, only zero address and the bootloader address have empty bytecode at the init
    // In the future, we might want to set all of the system contracts this way.
    ("", "EmptyContract", Address::zero(), ContractLanguage::Sol),
    (
        "",
        "EmptyContract",
        BOOTLOADER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "PubdataChunkPublisher",
        PUBDATA_CHUNK_PUBLISHER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "Create2Factory",
        CREATE2_FACTORY_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "L2GenesisUpgrade",
        L2_GENESIS_UPGRADE_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "L2Bridgehub",
        L2_BRIDGEHUB_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "L2MessageRoot",
        L2_MESSAGE_ROOT_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "L2AssetRouter",
        L2_ASSET_ROUTER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "L2NativeTokenVault",
        L2_NATIVE_TOKEN_VAULT_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "",
        "SloadContract",
        SLOAD_CONTRACT_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "L2WrappedBaseToken",
        L2_WRAPPED_BASE_TOKEN_IMPL,
        ContractLanguage::Sol,
    ),
    (
        "",
        "L2InteropRootStorage",
        L2_INTEROP_ROOT_STORAGE_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "L2MessageVerification",
        L2_MESSAGE_VERIFICATION_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "L2ChainAssetHandler",
        L2_CHAIN_ASSET_HANDLER_ADDRESS,
        ContractLanguage::Sol,
    ),
    (
        "../../l1-contracts/zkout/",
        "UpgradeableBeaconDeployer",
        UPGRADEABLE_BEACON_DEPLOYER_ADDRESS,
        ContractLanguage::Sol,
    ),
];

/// Gets default set of system contracts, based on Cargo workspace location.
pub fn get_system_smart_contracts() -> Vec<DeployedContract> {
    SYSTEM_CONTRACT_LIST
        .iter()
        .map(|(path, name, address, contract_lang)| DeployedContract {
            account_id: AccountTreeId::new(*address),
            bytecode: read_sys_contract_bytecode(path, name, contract_lang.clone()),
        })
        .collect()
}

/// Loads system contracts from a given directory.
pub fn get_system_smart_contracts_from_dir(path: PathBuf) -> Vec<DeployedContract> {
    let repo = SystemContractsRepo { root: path };
    SYSTEM_CONTRACT_LIST
        .iter()
        .map(|(path, name, address, contract_lang)| DeployedContract {
            account_id: AccountTreeId::new(*address),
            bytecode: repo.read_sys_contract_bytecode(path, name, None, contract_lang.clone()),
        })
        .collect::<Vec<_>>()
}
