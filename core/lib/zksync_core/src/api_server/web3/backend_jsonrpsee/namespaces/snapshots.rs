use crate::api_server::web3::backend_jsonrpsee::into_jsrpc_error;
use crate::api_server::web3::namespaces::SnapshotsNamespace;
use crate::l1_gas_price::L1GasPriceProvider;
use async_trait::async_trait;
use zksync_types::snapshots::{AllSnapshots, SnapshotHeader};
use zksync_types::L1BatchNumber;
use zksync_web3_decl::jsonrpsee::core::RpcResult;
use zksync_web3_decl::namespaces::SnapshotsNamespaceServer;

#[async_trait]
impl<G: L1GasPriceProvider + Send + Sync + 'static> SnapshotsNamespaceServer
    for SnapshotsNamespace<G>
{
    async fn get_all_snapshots(&self) -> RpcResult<AllSnapshots> {
        self.get_all_snapshots_impl()
            .await
            .map_err(into_jsrpc_error)
    }

    async fn get_snapshot_by_l1_batch_number(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> RpcResult<Option<SnapshotHeader>> {
        self.get_snapshot_by_l1_batch_number_impl(l1_batch_number)
            .await
            .map_err(into_jsrpc_error)
    }
}
