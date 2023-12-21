use zksync_types::{
    snapshots::{AllSnapshots, SnapshotHeader, SnapshotStorageLogsChunkMetadata},
    L1BatchNumber,
};
use zksync_web3_decl::error::Web3Error;

use crate::{
    api_server::web3::{backend_jsonrpsee::internal_error, metrics::API_METRICS, state::RpcState},
    l1_gas_price::L1GasPriceProvider,
};

#[derive(Debug)]
pub struct SnapshotsNamespace<G> {
    state: RpcState<G>,
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
        let method_name = "get_all_snapshots";
        let method_latency = API_METRICS.start_call(method_name);
        let mut storage_processor = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(method_name, err))?;
        let mut snapshots_dal = storage_processor.snapshots_dal();
        let response = snapshots_dal
            .get_all_snapshots()
            .await
            .map_err(|err| internal_error(method_name, err));
        method_latency.observe();
        response
    }

    pub async fn get_snapshot_by_l1_batch_number_impl(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<SnapshotHeader>, Web3Error> {
        let method_name = "get_snapshot_by_l1_batch_number";
        let method_latency = API_METRICS.start_call(method_name);
        let mut storage_processor = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(method_name, err))?;
        let mut snapshots_dal = storage_processor.snapshots_dal();
        let snapshot_metadata = snapshots_dal
            .get_snapshot_metadata(l1_batch_number)
            .await
            .map_err(|err| internal_error(method_name, err))?;
        if let Some(snapshot_metadata) = snapshot_metadata {
            let snapshot_files = snapshot_metadata.storage_logs_filepaths.clone();
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
                .map_err(|err| internal_error(method_name, err))?
                .unwrap();
            let miniblock_number = storage_processor
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(l1_batch_number)
                .await
                .map_err(|err| internal_error(method_name, err))?
                .unwrap()
                .1;
            method_latency.observe();
            Ok(Some(SnapshotHeader {
                l1_batch_number: snapshot_metadata.l1_batch_number,
                miniblock_number,
                last_l1_batch_with_metadata: l1_batch_with_metadata,
                storage_logs_chunks: chunks,
                factory_deps_filepath: snapshot_metadata.factory_deps_filepath,
            }))
        } else {
            method_latency.observe();
            Ok(None)
        }
    }
}
