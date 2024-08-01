#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::{
    snapshots::{AllSnapshots, SnapshotHeader},
    L1BatchNumber,
};

use crate::client::{ForEthereumLikeNetwork, L2};

#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "snapshots", client_bounds(Self: ForEthereumLikeNetwork<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "snapshots", client_bounds(Self: ForEthereumLikeNetwork<Net = L2>))
)]
pub trait SnapshotsNamespace {
    #[method(name = "getAllSnapshots")]
    async fn get_all_snapshots(&self) -> RpcResult<AllSnapshots>;

    #[method(name = "getSnapshot")]
    async fn get_snapshot_by_l1_batch_number(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> RpcResult<Option<SnapshotHeader>>;
}
