use std::convert::TryInto;

use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::{
    api,
    protocol_upgrade::{self, ProtocolUpgradeTx},
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion, VerifierParams, VersionPatch},
    H256,
};

#[derive(sqlx::FromRow)]
pub struct StorageProtocolVersion {
    pub minor: i32,
    pub patch: i32,
    pub timestamp: i64,
    pub recursion_scheduler_level_vk_hash: Vec<u8>,
    pub recursion_node_level_vk_hash: Vec<u8>,
    pub recursion_leaf_level_vk_hash: Vec<u8>,
    pub recursion_circuits_set_vks_hash: Vec<u8>,
    pub bootloader_code_hash: Vec<u8>,
    pub default_account_code_hash: Vec<u8>,
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
            params: VerifierParams {
                recursion_node_level_vk_hash: H256::from_slice(
                    &storage_version.recursion_node_level_vk_hash,
                ),
                recursion_leaf_level_vk_hash: H256::from_slice(
                    &storage_version.recursion_leaf_level_vk_hash,
                ),
                recursion_circuits_set_vks_hash: H256::from_slice(
                    &storage_version.recursion_circuits_set_vks_hash,
                ),
            },
            recursion_scheduler_level_vk_hash: H256::from_slice(
                &storage_version.recursion_scheduler_level_vk_hash,
            ),
        },
        base_system_contracts_hashes: BaseSystemContractsHashes {
            bootloader: H256::from_slice(&storage_version.bootloader_code_hash),
            default_aa: H256::from_slice(&storage_version.default_account_code_hash),
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
    pub upgrade_tx_hash: Option<Vec<u8>>,
}

impl From<StorageApiProtocolVersion> for api::ProtocolVersion {
    #[allow(deprecated)]
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
            l2_system_upgrade_tx_hash,
        )
    }
}
