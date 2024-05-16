use async_trait::async_trait;
use zksync_types::{
    snapshots::{AllSnapshots, SnapshotHeader},
    L1BatchNumber,
};
use zksync_web3_decl::{jsonrpsee::core::RpcResult, namespaces::SnapshotsNamespaceServer};

use crate::web3::namespaces::SnapshotsNamespace;

#[async_trait]
impl SnapshotsNamespaceServer for SnapshotsNamespace {
    async fn get_all_snapshots(&self) -> RpcResult<AllSnapshots> {
        self.get_all_snapshots_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_snapshot_by_l1_batch_number(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> RpcResult<Option<SnapshotHeader>> {
        self.get_snapshot_by_l1_batch_number_impl(l1_batch_number)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
