use std::str::FromStr;

use async_trait::async_trait;
use zksync_basic_types::{protocol_version::ProtocolVersionId, L1BatchNumber, L2BlockNumber, H256};
use zksync_snapshots_applier::{L1BlockMetadata, L2BlockMetadata, SnapshotsApplierMainNodeClient};
use zksync_types::{
    snapshots::{SnapshotHeader, SnapshotStorageLogsChunkMetadata, SnapshotVersion},
    tokens::TokenInfo,
};
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
        let l2_block_number = self
            .main_node_client
            .get_l2_block_range(l1_batch_number)
            .rpc_context("get_block_details")
            .with_arg("batch", &l1_batch_number)
            .await?
            .expect(&format!("Unable to find {l1_batch_number:?} batch"))
            .1;
        Ok(Some(SnapshotHeader {
            version: SnapshotVersion::Version1.into(),
            l1_batch_number,
            l2_block_number: L2BlockNumber(l2_block_number.as_u32()),
            storage_logs_chunks: vec![SnapshotStorageLogsChunkMetadata {
                chunk_id: 0,
                filepath: "".to_string(),
            }],
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
    pub root_hash: H256,
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
            timestamp: 0,
        }))
    }

    async fn fetch_l2_block_details(
        &self,
        _number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<L2BlockMetadata>> {
        Ok(Some(L2BlockMetadata {
            block_hash: Some(
                H256::from_str(
                    "0x2c20407b638b7e2cbe41a8af6be6c1cf3c8237e2f3ffff1fe674dd60aef5e772",
                )
                .unwrap(),
            ),
            protocol_version: Some(ProtocolVersionId::latest()),
            timestamp: 0,
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
        Ok(Some(SnapshotHeader {
            version: SnapshotVersion::Version1.into(),
            l1_batch_number,
            l2_block_number: L2BlockNumber(228),
            storage_logs_chunks: vec![SnapshotStorageLogsChunkMetadata {
                chunk_id: 0,
                filepath: "".to_string(),
            }],
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
