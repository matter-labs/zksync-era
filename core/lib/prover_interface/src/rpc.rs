use jsonrpsee::{
    core::{RpcResult, SubscriptionResult},
    proc_macros::rpc,
};
use zksync_types::{ChainAwareL1BatchNumber, L2ChainId};

use crate::api::{ProofGenerationData, SubmitProofRequest};

#[rpc(server, client)]
pub trait GatewayRpc {
    /// Submits proof generation data from client to server
    #[method(name = "submit_proof_generation_data")]
    async fn submit_proof_generation_data(
        &self,
        chain_id: L2ChainId,
        data: ProofGenerationData,
    ) -> RpcResult<()>;

    /// Notifies server that final proof was received, saved successfully and can be marked as `sent_to_server`
    #[method(name = "received_final_proof")]
    async fn received_final_proof(&self, batch_id: ChainAwareL1BatchNumber) -> RpcResult<()>;

    /// Subscription method, needs a chain id, for which subscription should be enabled
    #[subscription(name = "subscribe_for_proofs" => "subscription", unsubscribe = "unsubscribe_from_proofs", item = SubmitProofRequest)]
    async fn subscribe_for_proofs(&self, chain_id: L2ChainId) -> SubscriptionResult;
}
