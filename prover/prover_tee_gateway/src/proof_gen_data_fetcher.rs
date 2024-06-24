use async_trait::async_trait;
use secp256k1::Message;
use url::Url;
use zksync_prover_interface::{
    api::{
        SubmitProofResponse, SubmitTeeProofRequest, TeeProofGenerationDataRequest,
        TeeProofGenerationDataResponse,
    },
    outputs::L1BatchTeeProofForL1,
};
use zksync_tee_verifier::Verifiable;

use crate::api_data_fetcher::{PeriodicApi, PeriodicApiStruct};

#[async_trait]
impl PeriodicApi<TeeProofGenerationDataRequest> for PeriodicApiStruct {
    type JobId = ();
    type Response = TeeProofGenerationDataResponse;

    const SERVICE_NAME: &'static str = "TeeVerifierInputDataFetcher";

    async fn get_next_request(&self) -> Option<(Self::JobId, TeeProofGenerationDataRequest)> {
        Some(((), TeeProofGenerationDataRequest {}))
    }

    async fn send_request(
        &self,
        _: (),
        request: TeeProofGenerationDataRequest,
    ) -> reqwest::Result<Self::Response> {
        self.send_http_request(request, &self.api_url).await
    }

    async fn handle_response(&self, _: (), response: Self::Response) {
        match response {
            TeeProofGenerationDataResponse::Success(Some(tvi)) => {
                let tvi = *tvi;
                match tvi.verify() {
                    Err(e) => {
                        tracing::warn!("L1 batch verification failed: {e}")
                    }
                    Ok(verification_result) => {
                        let root_hash_bytes: [u8; 32] = verification_result.0.into();
                        let batch_number = verification_result.1;
                        let secret_key = self.key_pair.secret_key();
                        let msg_to_sign = Message::from_digest(root_hash_bytes);
                        let signature = secret_key.sign_ecdsa(msg_to_sign);
                        let request = SubmitTeeProofRequest(Box::new(L1BatchTeeProofForL1 {
                            signature: signature.serialize_compact().into(),
                            pubkey: self.key_pair.public_key().serialize().into(),
                            proof: root_hash_bytes.into(),
                        }));
                        // TODO(patrick) add better error handling
                        let mut submit_proof_endpoint =
                            Url::parse(self.submit_proof_endpoint.as_str()).unwrap();
                        submit_proof_endpoint
                            .path_segments_mut()
                            .unwrap()
                            .push(batch_number.to_string().as_str());
                        let _ = self
                            .send_http_request::<SubmitTeeProofRequest, SubmitProofResponse>(
                                request,
                                submit_proof_endpoint.to_string().as_str(),
                            )
                            .await;
                    }
                }
            }
            TeeProofGenerationDataResponse::Success(None) => {
                tracing::info!("There are currently no pending batches to be proven");
            }
            TeeProofGenerationDataResponse::Error(err) => {
                tracing::error!("Failed to get proof gen data: {:?}", err);
            }
        }
    }
}
