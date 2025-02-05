use jsonrpsee::core::{async_trait, RpcResult, SubscriptionResult};
use jsonrpsee::proc_macros::rpc;
use zksync_prover_interface::api::ProofGenerationData;
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
impl GatewayRpc for RpcServer {
    async fn submit_proof_generation_data(&self, data: ProofGenerationData) -> RpcResult<()> {
        self.state.save_proof_gen_data(data).await?;
        Ok(())
    }

    async fn subscribe_for_proofs(&self, chain_id: L2ChainId) -> SubscriptionResult {
        let mut subscription = self.subscribe_for_proofs(chain_id).await?;
        loop {
            if let Some((l1_batch_number, request)) = self.next_submit_proof_request(chain_id).await {
                subscription.send(l1_batch_number, request).await?;
                self.save_successful_sent_proof(l1_batch_number, chain_id).await;
            }
        }
    }
}