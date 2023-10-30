use crate::api_server::web3::state::RpcState;
use crate::l1_gas_price::L1GasPriceProvider;
use zksync_types::snapshots::{AllSnapshots, SnapshotFullInfo};
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
        Ok(snapshots_dal.get_snapshots().await.unwrap())
    }

    pub async fn get_snapshot_by_l1_batch_number_impl(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<SnapshotFullInfo>, Web3Error> {
        let mut storage_processor = self.state.connection_pool.access_storage().await.unwrap();
        let mut snapshots_dal = storage_processor.snapshots_dal();
        let snapshot_with_files = snapshots_dal
            .get_snapshot(L1BatchNumber(l1_batch_number.0))
            .await
            .unwrap();
        if snapshot_with_files.is_none() {
            Ok(None)
        } else {
            let snapshot_with_files = snapshot_with_files.as_ref().unwrap();
            let l1_batch_number = snapshot_with_files.metadata.l1_batch_number;
            let l1_batch_with_metadata = storage_processor
                .blocks_dal()
                .get_l1_batch_metadata(l1_batch_number)
                .await
                .unwrap()
                .unwrap();
            let miniblock_number = storage_processor
                .storage_logs_snapshots_dal()
                .get_last_miniblock_number(l1_batch_number)
                .await
                .unwrap();
            Ok(Some(SnapshotFullInfo {
                metadata: snapshot_with_files.metadata.clone(),
                miniblock_number,
                storage_logs_files: snapshot_with_files.storage_logs_files.clone(),
                last_l1_batch_with_metadata: l1_batch_with_metadata,
            }))
        }
    }
}
