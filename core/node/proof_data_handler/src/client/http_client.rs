use zksync_prover_interface::{api::ProofGenerationData, outputs::L1BatchProofForL1};
use zksync_types::L1BatchNumber;

pub(crate) struct HttpClient {
    api_url: String,
    client: reqwest::Client,
}

impl HttpClient {
    pub(crate) fn new(
        api_url: String,
    ) -> Self {
        Self {
            api_url,
            client: reqwest::ClientBuilder::new().use_rustls_tls().https_only(true).build().unwrap(),
        }
    }

    pub(crate) async fn send_proof_generation_data(&self, _data: ProofGenerationData) -> Result<(), reqwest::Error> {
        unimplemented!()
    }

    pub(crate) async fn fetch_proof(&self) -> Result<Option<(L1BatchNumber, L1BatchProofForL1)>, reqwest::Error> {
        unimplemented!()
    }

    pub(crate) async fn received_final_proof_request(&self, _l1_batch_number: L1BatchNumber) -> Result<(), reqwest::Error> {
        unimplemented!()
    }
}
