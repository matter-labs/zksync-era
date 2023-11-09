use crate::api_server::web3::state::RpcState;
use crate::l1_gas_price::L1GasPriceProvider;
use zksync_types::snapshots::{AllSnapshots, SnapshotHeader, SnapshotStorageLogsChunkMetadata};
use zksync_types::L1BatchNumber;
use zksync_web3_decl::error::Web3Error;

#[derive(Debug)]
pub struct SnapshotsNamespace<G> {
    pub state: RpcState<G>,
}

impl<G> Clone for SnapshotsNamespace<G> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}
impl<G: L1GasPriceProvider> SnapshotsNamespace<G> {
    pub fn new(state: RpcState<G>) -> Self {
        Self { state }
    }
    pub async fn get_all_snapshots_impl(&self) -> Result<AllSnapshots, Web3Error> {
        let mut storage_processor = self.state.connection_pool.access_storage().await.unwrap();
        let mut snapshots_dal = storage_processor.snapshots_dal();
        Ok(snapshots_dal.get_all_snapshots().await.unwrap())
    }

    pub async fn get_snapshot_by_l1_batch_number_impl(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<SnapshotHeader>, Web3Error> {
        let mut storage_processor = self.state.connection_pool.access_storage().await.unwrap();
        let mut snapshots_dal = storage_processor.snapshots_dal();
        let snapshot_files = snapshots_dal
            .get_snapshot_files(l1_batch_number)
            .await
            .unwrap();
        if snapshot_files.is_none() {
            Ok(None)
        } else {
            let snapshot_metadata = snapshots_dal
                .get_snapshot_metadata(l1_batch_number)
                .await
                .unwrap()
                .unwrap();
            let snapshot_files = snapshot_files.as_ref().unwrap();
            let chunks = snapshot_files
                .iter()
                .enumerate()
                .map(|(chunk_id, filepath)| SnapshotStorageLogsChunkMetadata {
                    chunk_id: chunk_id as u64,
                    filepath: filepath.clone(),
                })
                .collect();
            let l1_batch_with_metadata = storage_processor
                .blocks_dal()
                .get_l1_batch_metadata(l1_batch_number)
                .await
                .unwrap()
                .unwrap();
            let miniblock_number = storage_processor
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(l1_batch_number)
                .await
                .unwrap()
                .unwrap()
                .1;
            Ok(Some(SnapshotHeader {
                l1_batch_number: snapshot_metadata.l1_batch_number,
                generated_at: snapshot_metadata.generated_at,
                miniblock_number,
                last_l1_batch_with_metadata: l1_batch_with_metadata,
                storage_logs_chunks: chunks,
                factory_deps_filepath: snapshot_metadata.factory_deps_filepath,
            }))
        }
    }
}
