use crate::handlers::fri_handlers::PutFriProof;
use crate::state::LinkedProof;
use anyhow::Context;
use reqwest::Url;
use zksync_airbender_execution_utils::ProgramProof;

pub struct ProofCacheClient {
    url: Url,
    client: reqwest::Client,
}

impl ProofCacheClient {
    pub fn new(url: String) -> anyhow::Result<Self> {
        let url = Url::parse(&url).map_err(|e| anyhow::anyhow!("Invalid URL: {e}"))?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(1))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {e}"))?;

        Ok(Self { url, client })
    }

    pub async fn put_fri(&self, block_number: &str, proof: ProgramProof) -> anyhow::Result<()> {
        let url = self.url.join("/fri_proofs/").context("Failed to create URL")?;
        tracing::info!("url = {url}");
        let payload = PutFriProof {
            block_number: block_number.to_string(),
            proof,
        };
        let response = self.client.post(url).json(&payload).send().await.context("Request failed")?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to put proof: {}", response.status()))
        }
    }

    pub async fn get_fri(&self, block_number: &str) -> anyhow::Result<Option<ProgramProof>> {
        let url = self.url.join(&format!("/fri_proofs/{block_number}")).context("Failed to create URL")?;

        let response = self.client.get(url).send().await.context("Request failed")?;
        match response.status() {
            reqwest::StatusCode::OK => {
                let proof = response.json::<ProgramProof>().await.context("Failed to read response text")?;
                Ok(Some(proof))
            }
            reqwest::StatusCode::NOT_FOUND => Ok(None),
            e => Err(anyhow::anyhow!("Failed to get proof: {e}")),
        }
    }

    pub async fn put_linked(&self, linked_proof: LinkedProof) -> anyhow::Result<()> {
        let url = self.url.join("/linked_proofs/").context("Failed to create URL")?;
        tracing::info!("url = {url}");
        let response = self.client.post(url).json(&linked_proof).send().await.context("Request failed")?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to put linked proof: {}", response.status()))
        }
    }

    pub async fn get_linked(&self) -> anyhow::Result<LinkedProof> {
        let url = self.url.join("/linked_proofs/").context("Failed to create URL")?;

        let response = self.client.get(url).send().await.context("Request failed")?;
        if response.status().is_success() {
            response.json::<LinkedProof>().await.context("Failed to read response text")
        } else {
            Err(anyhow::anyhow!("Failed to get linked proof: {}", response.status()))
        }
    }
}