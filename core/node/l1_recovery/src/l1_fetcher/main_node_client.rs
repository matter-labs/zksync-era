use async_trait::async_trait;
use zksync_basic_types::{protocol_version::ProtocolVersionId, L1BatchNumber, L2BlockNumber, H256};
use zksync_snapshots_applier::{L1BlockMetadata, L2BlockMetadata, SnapshotsApplierMainNodeClient};
use zksync_types::{
    snapshots::{SnapshotHeader, SnapshotStorageLogsChunkMetadata, SnapshotVersion},
    tokens::TokenInfo,
};
use zksync_vm_interface::L2Block;
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientResult},
    namespaces::ZksNamespaceClient,
};

#[derive(Debug, Clone)]
pub struct L1RecoveryOnlineMainNodeClient {
    pub newest_l1_batch_number: L1BatchNumber,
    pub root_hash: H256,
    pub main_node_client: Box<DynClient<L2>>,
    pub chunks_count: u64,
}

#[async_trait]
impl SnapshotsApplierMainNodeClient for L1RecoveryOnlineMainNodeClient {
    async fn fetch_l1_batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<L1BlockMetadata>> {
        self.main_node_client.fetch_l1_batch_details(number).await
    }

    async fn fetch_l2_block_details(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<L2BlockMetadata>> {
        self.main_node_client.fetch_l2_block_details(number).await
    }

    async fn fetch_newest_snapshot_l1_batch_number(
        &self,
    ) -> EnrichedClientResult<Option<L1BatchNumber>> {
        Ok(Some(self.newest_l1_batch_number))
    }

    async fn fetch_snapshot(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<SnapshotHeader>> {
        assert_eq!(l1_batch_number, self.newest_l1_batch_number);
        let chunks: Vec<SnapshotStorageLogsChunkMetadata> = (0..self.chunks_count)
            .map(|chunk_id| SnapshotStorageLogsChunkMetadata {
                chunk_id,
                filepath: "".to_string(),
            })
            .collect();
        let l2_block_number = self
            .main_node_client
            .get_l2_block_range(l1_batch_number)
            .rpc_context("get_block_details")
            .with_arg("batch", &l1_batch_number)
            .await?
            .unwrap_or_else(|| panic!("Unable to find {l1_batch_number:?} batch"))
            .1;
        Ok(Some(SnapshotHeader {
            version: SnapshotVersion::Version1.into(),
            l1_batch_number,
            l2_block_number: L2BlockNumber(l2_block_number.as_u32()),
            storage_logs_chunks: chunks,
            factory_deps_filepath: "".to_string(),
        }))
    }

    async fn fetch_tokens(
        &self,
        _at_l2_block: L2BlockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>> {
        self.main_node_client.fetch_tokens(_at_l2_block).await
    }
}

#[derive(Debug, Clone)]
pub struct L1RecoveryDetachedMainNodeClient {
    pub newest_l1_batch_number: L1BatchNumber,
    pub newest_l1_batch_root_hash: H256,
    pub newest_l1_batch_timestamp: u64,
    pub newest_l2_block: L2Block,
    pub root_hash: H256,
    pub chunks_count: u64,
}

#[async_trait]
impl SnapshotsApplierMainNodeClient for L1RecoveryDetachedMainNodeClient {
    async fn fetch_l1_batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<L1BlockMetadata>> {
        assert_eq!(self.newest_l1_batch_number, number);
        Ok(Some(L1BlockMetadata {
            root_hash: Some(self.root_hash),
            timestamp: self.newest_l1_batch_timestamp,
        }))
    }

    async fn fetch_l2_block_details(
        &self,
        _number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<L2BlockMetadata>> {
        Ok(Some(L2BlockMetadata {
            block_hash: Some(self.newest_l2_block.hash),
            protocol_version: Some(ProtocolVersionId::latest()),
            timestamp: self.newest_l2_block.timestamp,
        }))
    }

    async fn fetch_newest_snapshot_l1_batch_number(
        &self,
    ) -> EnrichedClientResult<Option<L1BatchNumber>> {
        Ok(Some(self.newest_l1_batch_number))
    }

    async fn fetch_snapshot(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<SnapshotHeader>> {
        assert_eq!(self.newest_l1_batch_number, l1_batch_number);
        let chunks: Vec<SnapshotStorageLogsChunkMetadata> = (0..self.chunks_count)
            .map(|chunk_id| SnapshotStorageLogsChunkMetadata {
                chunk_id,
                filepath: "".to_string(),
            })
            .collect();
        Ok(Some(SnapshotHeader {
            version: SnapshotVersion::Version1.into(),
            l1_batch_number,
            l2_block_number: L2BlockNumber(self.newest_l2_block.number),
            storage_logs_chunks: chunks,
            factory_deps_filepath: "".to_string(),
        }))
    }

    async fn fetch_tokens(
        &self,
        _at_l2_block: L2BlockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>> {
        Ok(vec![])
    }
}
