mod state;
mod methods;

use jsonrpsee::proc_macros::rpc;
use jsonrpsee::RpcModule;
use jsonrpsee::server::Server;
use tokio::sync::watch;
use zksync_config::configs::GatewayConfig;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_interface::api::ProofGenerationData;

pub struct RpcServer{
    pub(crate) state: state::RpcState,
    pub(crate) ws_port: u16,
}

impl RpcServer {
    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()>{
        let address = format!("127.0.0.1:{}", self.ws_port);
        let server = Server::builder().build(address).await?;
        let handle = server.start(self.module);

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

