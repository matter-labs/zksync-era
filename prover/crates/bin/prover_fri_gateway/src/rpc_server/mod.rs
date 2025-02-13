mod processor;
use std::time::Duration;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::{PendingSubscriptionSink, RpcModule, SubscriptionMessage, TrySendError};
use jsonrpsee::core::{async_trait, RpcResult, SubscriptionResult};
use jsonrpsee::server::Server;
use tokio::sync::watch;
use zksync_config::configs::GatewayConfig;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_interface::api::ProofGenerationData;
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_prover_interface::rpc::GatewayRpcServer;
use zksync_types::L2ChainId;
use crate::rpc_server::processor::RpcDataProcessor;

pub struct RpcServer {
    pub(crate) processor: RpcDataProcessor,
    pub(crate) ws_port: u16,
}

impl RpcServer {
    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let address = format!("127.0.0.1:{}", self.ws_port);
        let server = Server::builder().build(address).await?;
        let handle = server.start(self.processor.into_rpc());

        tokio::spawn(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop signal sender for JSON-RPC server was dropped \
                     without sending a signal"
                );
            }

            handle.stop().ok()
        });

        handle.stopped().await?
    }
}
