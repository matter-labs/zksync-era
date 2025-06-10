// TODO: Add testing around this

use anyhow::anyhow;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use zkos_wrapper::SnarkWrapperProof;
use zksync_airbender_execution_utils::ProgramProof;
use zksync_types::L2BlockNumber;

#[derive(Debug, Serialize, Deserialize)]
struct ProverJobPayload {
    block_number: u32,
    prover_input: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProofPayload {
    block_number: u32,
    proof: String,
}

#[derive(Debug)]
pub struct SequencerProofClient {
    client: reqwest::Client,
    url: String,
}

impl SequencerProofClient {
    pub fn new(sequencer_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: sequencer_url,
        }
    }

    pub fn sequencer_url(&self) -> &str {
        &self.url
    }

    pub async fn pick_snark_job(&self) -> anyhow::Result<Option<(ProgramProof, L2BlockNumber)>> {
        let url = format!("{}/prover-jobs/SNARK/pick", self.url);
        let resp = self.client.post(&url).send().await?;
        match resp.status() {
            StatusCode::OK => {
                let ProverJobPayload { block_number, prover_input } = resp.json::<ProverJobPayload>().await?;
                let proof = bincode::deserialize(&base64::decode(prover_input)?)?;
                Ok(Some((proof, L2BlockNumber(block_number))))
            }
            StatusCode::NO_CONTENT => {
                Ok(None)
            }
            _ => {
                Err(anyhow!("Failed to pick SNARK job: {:?}", resp))
            }
        }
    }

    pub async fn submit_snark_proof(&self, block_number: L2BlockNumber, proof: SnarkWrapperProof) -> anyhow::Result<()> {
        let url = format!("{}/prover-jobs/SNARK/submit", self.url);
        let payload = ProofPayload {
            block_number: block_number.0,
            proof: base64::encode(bincode::serialize(&proof)?),
        };
        self.client
            .post(&url)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}
