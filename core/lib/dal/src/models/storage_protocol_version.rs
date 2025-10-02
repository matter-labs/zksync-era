use std::convert::TryInto;

use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::{
    api,
    protocol_upgrade::{self, ProtocolUpgradeTx},
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion, VersionPatch},
    H256,
};

#[derive(sqlx::FromRow)]
pub struct StorageProtocolVersion {
    pub minor: i32,
    pub patch: i32,
    pub timestamp: i64,
    pub snark_wrapper_vk_hash: Vec<u8>,
    pub fflonk_snark_wrapper_vk_hash: Option<Vec<u8>>,
    pub bootloader_code_hash: Vec<u8>,
    pub default_account_code_hash: Vec<u8>,
    pub evm_emulator_code_hash: Option<Vec<u8>>,
}

pub(crate) fn protocol_version_from_storage(
    storage_version: StorageProtocolVersion,
    tx: Option<ProtocolUpgradeTx>,
) -> protocol_upgrade::ProtocolVersion {
    protocol_upgrade::ProtocolVersion {
        version: ProtocolSemanticVersion {
            minor: (storage_version.minor as u16).try_into().unwrap(),
            patch: VersionPatch(storage_version.patch as u32),
        },
        timestamp: storage_version.timestamp as u64,
        l1_verifier_config: L1VerifierConfig {
            snark_wrapper_vk_hash: H256::from_slice(&storage_version.snark_wrapper_vk_hash),
            fflonk_snark_wrapper_vk_hash: storage_version
                .fflonk_snark_wrapper_vk_hash
                .as_ref()
                .map(|x| H256::from_slice(x)),
        },
        base_system_contracts_hashes: BaseSystemContractsHashes {
            bootloader: H256::from_slice(&storage_version.bootloader_code_hash),
            default_aa: H256::from_slice(&storage_version.default_account_code_hash),
            evm_emulator: storage_version
                .evm_emulator_code_hash
                .as_deref()
                .map(H256::from_slice),
        },
        tx,
    }
}

#[derive(sqlx::FromRow)]
pub struct StorageApiProtocolVersion {
    pub minor: i32,
    pub timestamp: i64,
    pub bootloader_code_hash: Vec<u8>,
    pub default_account_code_hash: Vec<u8>,
    pub evm_emulator_code_hash: Option<Vec<u8>>,
    pub upgrade_tx_hash: Option<Vec<u8>>,
}

#[allow(deprecated)]
impl From<StorageApiProtocolVersion> for api::ProtocolVersion {
    fn from(storage_protocol_version: StorageApiProtocolVersion) -> Self {
        let l2_system_upgrade_tx_hash = storage_protocol_version
            .upgrade_tx_hash
            .as_ref()
            .map(|hash| H256::from_slice(hash));
        api::ProtocolVersion::new(
            storage_protocol_version.minor as u16,
            storage_protocol_version.timestamp as u64,
            H256::from_slice(&storage_protocol_version.bootloader_code_hash),
            H256::from_slice(&storage_protocol_version.default_account_code_hash),
            storage_protocol_version
                .evm_emulator_code_hash
                .as_deref()
                .map(H256::from_slice),
            l2_system_upgrade_tx_hash,
        )
    }
}

impl From<StorageApiProtocolVersion> for api::ProtocolVersionInfo {
    fn from(storage_protocol_version: StorageApiProtocolVersion) -> Self {
        let l2_system_upgrade_tx_hash = storage_protocol_version
            .upgrade_tx_hash
            .as_ref()
            .map(|hash| H256::from_slice(hash));
        api::ProtocolVersionInfo {
            minor_version: storage_protocol_version.minor as u16,
            timestamp: storage_protocol_version.timestamp as u64,
            bootloader_code_hash: H256::from_slice(&storage_protocol_version.bootloader_code_hash),
            default_account_code_hash: H256::from_slice(
                &storage_protocol_version.default_account_code_hash,
            ),
            evm_emulator_code_hash: storage_protocol_version
                .evm_emulator_code_hash
                .as_deref()
                .map(H256::from_slice),
            l2_system_upgrade_tx_hash,
        }
    }
}
