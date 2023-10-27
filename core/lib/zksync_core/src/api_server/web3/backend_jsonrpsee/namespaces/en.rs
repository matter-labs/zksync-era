use zksync_types::{
    api::en::{Heartbeat, SyncBlock},
    MiniblockNumber,
};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::en::EnNamespaceServer,
};

use crate::{
    api_server::web3::{backend_jsonrpsee::into_jsrpc_error, namespaces::EnNamespace},
    l1_gas_price::L1GasPriceProvider,
    metrics::EN_HEARTBEAT_METRICS,
};

#[async_trait]
impl<G: L1GasPriceProvider + Send + Sync + 'static> EnNamespaceServer for EnNamespace<G> {
    async fn sync_l2_block(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> RpcResult<Option<SyncBlock>> {
        self.sync_l2_block_impl(block_number, include_transactions)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn send_heartbeat(&self, heartbeat: Heartbeat) -> RpcResult<()> {
        match heartbeat {
            Heartbeat::V1(hb_v1) => {
                EN_HEARTBEAT_METRICS.versions[&(
                    hb_v1.name.clone(),
                    format!("{}", hb_v1.server_version),
                    hb_v1.protocol_version as u16,
                )]
                    .set(1);

                EN_HEARTBEAT_METRICS.executed_l1_batch_number[&hb_v1.name]
                    .set(hb_v1.executed_l1_batch_number.0 as u64);

                EN_HEARTBEAT_METRICS.last_known_l1_batch_number[&hb_v1.name]
                    .set(hb_v1.last_known_l1_batch_number.0 as u64);
            }
            h => {
                tracing::warn!("unexpected version of heartbeat {h:?}");
            }
        }
        Ok(())
    }
}
