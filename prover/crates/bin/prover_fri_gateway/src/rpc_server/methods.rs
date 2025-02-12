use jsonrpsee::core::{async_trait, RpcResult, SubscriptionResult};
use jsonrpsee::PendingSubscriptionSink;
use jsonrpsee::proc_macros::rpc;
use zksync_prover_interface::api::ProofGenerationData;
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_types::L2ChainId;
use crate::rpc_server::RpcServer;
use crate::rpc_server::state::RpcState;

#[rpc(server)]
pub trait GatewayRpc {
    #[method(name = "submit_proof_generation_data")]
    async fn submit_proof_generation_data(&self, data: ProofGenerationData) -> RpcResult<()>;

    #[subscription(name = "subscribe_for_proofs" => "subscription", unsubscribe = "unsubscribe_from_proofs", item = L1BatchProofForL1)]
    async fn subscribe_for_proofs(&self, chain_id: L2ChainId) -> SubscriptionResult;
}

#[async_trait]
impl GatewayRpcServer for RpcServer {
    async fn submit_proof_generation_data(&self, data: ProofGenerationData) -> RpcResult<()> {
        self.state.save_proof_gen_data(data).await?;
        Ok(())
    }

    async fn subscribe_for_proofs(&self, subscription_sink: PendingSubscriptionSink, chain_id: L2ChainId) -> SubscriptionResult {
        self.sub(subscription_sink, chain_id).await;
        Ok(())
    }
}

#[derive(Debug)]
pub enum SubscriptionEvent{
    Subscribed(L2ChainId),
    ProofGenerated(L2ChainId, L1BatchProofForL1),
}