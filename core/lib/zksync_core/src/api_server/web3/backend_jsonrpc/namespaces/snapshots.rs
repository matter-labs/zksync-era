// Built-in uses

// External uses
use jsonrpc_core::{BoxFuture, Result};
use jsonrpc_derive::rpc;

// Workspace uses
use crate::api_server::web3::backend_jsonrpc::error::into_jsrpc_error;
use crate::l1_gas_price::L1GasPriceProvider;
use zksync_types::snapshots::{AllSnapshots, SnapshotHeader};
use zksync_types::L1BatchNumber;

// Local uses
use crate::web3::namespaces::SnapshotsNamespace;

#[rpc]
pub trait SnapshotsNamespaceT {
    #[rpc(name = "snapshots_getAllSnapshots")]
    fn get_all_snapshots(&self) -> BoxFuture<Result<AllSnapshots>>;

    #[rpc(name = "snapshots_getSnapshot")]
    fn get_snapshot_by_l1_batch_number(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> BoxFuture<Result<Option<SnapshotHeader>>>;
}

impl<G: L1GasPriceProvider + Send + Sync + 'static> SnapshotsNamespaceT for SnapshotsNamespace<G> {
    fn get_all_snapshots(&self) -> BoxFuture<Result<AllSnapshots>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_all_snapshots_impl()
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_snapshot_by_l1_batch_number(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> BoxFuture<Result<Option<SnapshotHeader>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_snapshot_by_l1_batch_number_impl(l1_batch_number)
                .await
                .map_err(into_jsrpc_error)
        })
    }
}
