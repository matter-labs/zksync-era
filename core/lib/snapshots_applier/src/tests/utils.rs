//! Test utils.

use std::collections::HashMap;

use async_trait::async_trait;
use zksync_types::{
    api::en::SyncBlock,
    block::L1BatchHeader,
    commitment::{L1BatchMetaParameters, L1BatchMetadata, L1BatchWithMetadata},
    snapshots::{SnapshotHeader, SnapshotStorageLog},
    AccountTreeId, L1BatchNumber, MiniblockNumber, ProtocolVersionId, StorageKey, StorageValue,
    H160, H256,
};
use zksync_web3_decl::jsonrpsee::core::ClientError as RpcError;

use crate::SnapshotsApplierMainNodeClient;

#[derive(Debug, Default)]
pub(super) struct MockMainNodeClient {
    pub fetch_l2_block_responses: HashMap<MiniblockNumber, SyncBlock>,
    pub fetch_newest_snapshot_response: Option<SnapshotHeader>,
}

#[async_trait]
impl SnapshotsApplierMainNodeClient for MockMainNodeClient {
    async fn fetch_l2_block(&self, number: MiniblockNumber) -> Result<Option<SyncBlock>, RpcError> {
        Ok(self.fetch_l2_block_responses.get(&number).cloned())
    }

    async fn fetch_newest_snapshot(&self) -> Result<Option<SnapshotHeader>, RpcError> {
        Ok(self.fetch_newest_snapshot_response.clone())
    }
}

pub(crate) fn miniblock_metadata(
    number: MiniblockNumber,
    l1_batch_number: L1BatchNumber,
    hash: H256,
) -> SyncBlock {
    SyncBlock {
        number,
        l1_batch_number,
        last_in_batch: true,
        timestamp: 0,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        fair_pubdata_price: None,
        base_system_contracts_hashes: Default::default(),
        operator_address: Default::default(),
        transactions: None,
        virtual_blocks: None,
        hash: Some(hash),
        protocol_version: Default::default(),
    }
}

pub(crate) fn l1_block_metadata(
    l1_batch_number: L1BatchNumber,
    root_hash: H256,
) -> L1BatchWithMetadata {
    L1BatchWithMetadata {
        header: L1BatchHeader::new(
            l1_batch_number,
            0,
            Default::default(),
            ProtocolVersionId::default(),
        ),
        metadata: L1BatchMetadata {
            root_hash,
            rollup_last_leaf_index: 0,
            merkle_root_hash: H256::zero(),
            initial_writes_compressed: vec![],
            repeated_writes_compressed: vec![],
            commitment: H256::zero(),
            l2_l1_messages_compressed: vec![],
            l2_l1_merkle_root: H256::zero(),
            block_meta_params: L1BatchMetaParameters {
                zkporter_is_available: false,
                bootloader_code_hash: H256::zero(),
                default_aa_code_hash: H256::zero(),
            },
            aux_data_hash: H256::zero(),
            meta_parameters_hash: H256::zero(),
            pass_through_data_hash: H256::zero(),
            events_queue_commitment: None,
            bootloader_initial_content_commitment: None,
            state_diffs_compressed: vec![],
        },
        factory_deps: vec![],
    }
}

pub(super) fn random_storage_logs(
    l1_batch_number: L1BatchNumber,
    chunk_id: u64,
    logs_per_chunk: u64,
) -> Vec<SnapshotStorageLog> {
    (0..logs_per_chunk)
        .map(|x| SnapshotStorageLog {
            key: StorageKey::new(
                AccountTreeId::from_fixed_bytes(H160::random().to_fixed_bytes()),
                H256::random(),
            ),
            value: StorageValue::random(),
            l1_batch_number_of_initial_write: l1_batch_number,
            enumeration_index: x + chunk_id * logs_per_chunk,
        })
        .collect()
}
