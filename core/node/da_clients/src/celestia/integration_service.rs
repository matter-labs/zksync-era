use eq_common::eqs::inclusion_client::InclusionClient;
use eq_common::eqs::{GetKeccakInclusionRequest, GetKeccakInclusionResponse};
use tonic::transport::Channel;
use crate::celestia::client::BlobId;
use tonic::Status as TonicStatus;

pub (crate) struct IntegrationClient {
    grpc_channel: Channel,
}

impl IntegrationClient {
    pub (crate) fn new(grpc_channel: Channel) -> Self {
        Self {
            grpc_channel,
        }
    }

    pub async fn get_keccak_inclusion(&self, request: &BlobId) -> Result<GetKeccakInclusionResponse, TonicStatus> {
        let request = GetKeccakInclusionRequest {
            commitment: request.commitment.0.to_vec(),
            namespace: request.namespace.0.to_vec(),
            height: request.height,
        };
        let mut client = InclusionClient::new(self.grpc_channel.clone());
        match client.get_keccak_inclusion(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(e),
        }
    }
}
