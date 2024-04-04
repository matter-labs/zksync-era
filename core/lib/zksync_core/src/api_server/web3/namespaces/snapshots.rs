use anyhow::Context as _;
use zksync_dal::CoreDal;
use zksync_types::{
    snapshots::{AllSnapshots, SnapshotHeader, SnapshotStorageLogsChunkMetadata},
    L1BatchNumber,
};
use zksync_web3_decl::error::Web3Error;

use crate::api_server::web3::{backend_jsonrpsee::MethodTracer, state::RpcState};

#[derive(Debug, Clone)]
pub(crate) struct SnapshotsNamespace {
    state: RpcState,
}

impl SnapshotsNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn get_all_snapshots_impl(&self) -> Result<AllSnapshots, Web3Error> {
        let mut storage_processor = self.state.connection_pool.connection_tagged("api").await?;
        let mut snapshots_dal = storage_processor.snapshots_dal();
        Ok(snapshots_dal
            .get_all_complete_snapshots()
            .await
            .context("get_all_complete_snapshots")?)
    }

    pub async fn get_snapshot_by_l1_batch_number_impl(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<SnapshotHeader>, Web3Error> {
        let mut storage_processor = self.state.connection_pool.connection_tagged("api").await?;
        let snapshot_metadata = storage_processor
            .snapshots_dal()
            .get_snapshot_metadata(l1_batch_number)
            .await
            .context("get_snapshot_metadata")?;

        let Some(snapshot_metadata) = snapshot_metadata else {
            return Ok(None);
        };

        let snapshot_files = snapshot_metadata.storage_logs_filepaths;
        let is_complete = snapshot_files.iter().all(Option::is_some);
        if !is_complete {
            // We don't return incomplete snapshots via API.
            return Ok(None);
        }

        let chunks = snapshot_files
            .into_iter()
            .enumerate()
            .filter_map(|(chunk_id, filepath)| {
                Some(SnapshotStorageLogsChunkMetadata {
                    chunk_id: chunk_id as u64,
                    filepath: filepath?,
                })
            })
            .collect();
        let (_, miniblock_number) = storage_processor
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await
            .context("get_miniblock_range_of_l1_batch")?
            .with_context(|| format!("missing miniblocks for L1 batch #{l1_batch_number}"))?;

        Ok(Some(SnapshotHeader {
            version: snapshot_metadata.version.into(),
            l1_batch_number: snapshot_metadata.l1_batch_number,
            miniblock_number,
            storage_logs_chunks: chunks,
            factory_deps_filepath: snapshot_metadata.factory_deps_filepath,
        }))
    }
}
