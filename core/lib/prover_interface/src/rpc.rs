use jsonrpsee::core::{RpcResult, SubscriptionResult};
use jsonrpsee::proc_macros::rpc;
use zksync_types::L2ChainId;
use crate::api::ProofGenerationData;
use crate::outputs::L1BatchProofForL1;

#[rpc(server, client)]
pub trait GatewayRpc {
    #[method(name = "submit_proof_generation_data")]
    async fn submit_proof_generation_data(&self, data: ProofGenerationData) -> RpcResult<()>;

    #[subscription(name = "subscribe_for_proofs" => "subscription", unsubscribe = "unsubscribe_from_proofs", item = L1BatchProofForL1)]
    async fn subscribe_for_proofs(&self, chain_id: L2ChainId) -> SubscriptionResult;
}