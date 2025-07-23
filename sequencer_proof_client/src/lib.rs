// TODO: Add testing around this

use anyhow::{anyhow, Context};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use zkos_wrapper::SnarkWrapperProof;
use zksync_airbender_execution_utils::ProgramProof;
use zksync_types::L2BlockNumber;

#[derive(Debug, Serialize, Deserialize)]
struct GetSnarkProofPayload {
    block_number_from: u64,
    block_number_to: u64,
    fri_proofs: Vec<String>, // base64‑encoded FRI proofs
}


#[derive(Debug, Serialize, Deserialize)]
struct SubmitSnarkProofPayload {
    block_number_from: u64,
    block_number_to: u64,
    proof: String, // base64‑encoded FRI proofs
}

impl TryInto<SnarkProofInputs> for GetSnarkProofPayload {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<SnarkProofInputs, Self::Error> {
        let mut fri_proofs = vec![];
        for encoded_proof in self.fri_proofs {
            let fri_proof = bincode::deserialize(&base64::decode(encoded_proof)?)?;
            fri_proofs.push(fri_proof);
        }

        Ok(SnarkProofInputs {
            from_block_number: L2BlockNumber(self.block_number_from.try_into().expect("block_number_from should fit into L2BlockNumber(u32)")),
            to_block_number: L2BlockNumber(self.block_number_to.try_into().expect("block_number_to should fit into L2BlockNumber(u32)")),
            fri_proofs,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnarkProofInputs {
    pub from_block_number: L2BlockNumber,
    pub to_block_number: L2BlockNumber,
    pub fri_proofs: Vec<ProgramProof>,
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

    pub async fn pick_snark_job(&self) -> anyhow::Result<Option<SnarkProofInputs>> {
        let url = format!("{}/prover-jobs/SNARK/pick", self.url);
        let resp = self.client.post(&url).send().await?;
        match resp.status() {
            StatusCode::OK => {
                let get_snark_proof_payload = resp.json::<GetSnarkProofPayload>().await?;
                Ok(Some(get_snark_proof_payload.try_into().context("failed to parse SnarkProofPayload")?))
            }
            StatusCode::NO_CONTENT => {
                Ok(None)
            }
            _ => {
                Err(anyhow!("Failed to pick SNARK job: {:?}", resp))
            }
        }
    }

    pub async fn submit_snark_proof(&self, from_block_number: L2BlockNumber, to_block_number: L2BlockNumber, proof: SnarkWrapperProof) -> anyhow::Result<()> {
        let url = format!("{}/prover-jobs/SNARK/submit", self.url);
        let payload = SubmitSnarkProofPayload {
            block_number_from: from_block_number.0 as u64,
            block_number_to: to_block_number.0 as u64,
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