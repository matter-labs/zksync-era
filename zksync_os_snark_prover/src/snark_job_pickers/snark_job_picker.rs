use crate::snark_executor::SnarkExecutor;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use zkos_wrapper::SnarkWrapperProof;
use zksync_airbender_execution_utils::ProgramProof;
use zksync_prover_job_processor::JobPicker;
use zksync_sequencer_proof_client::SequencerProofClient;
use zksync_types::{L2BlockNumber};

// #[derive(Debug, Serialize, Deserialize)]
// struct NextProverJobPayload {
//     block_number: u32,
//     prover_input: String, // base64-encoded
// }
//
// #[derive(Debug, Serialize, Deserialize)]
// struct ProofPayload {
//     block_number: u32,
//     proof: String, // base64-encoded
// }
//
// #[derive(Debug, Clone)]
// pub struct SequencerClient {
//     http_client: Client,
//     sequencer_url: String,
// }
//
// impl SequencerClient {
//     pub fn new(sequencer_url: String) -> Self {
//         Self {
//             http_client: Client::new(),
//             sequencer_url,
//         }
//     }
//
//     pub async fn pick_snark_job(&self) -> anyhow::Result<Option<NextProverJobPayload>> {
//         let url = format!("{}/prover-jobs/SNARK/pick", self.sequencer_url);
//         // let resp = self.http_client.post(&url).send().await?;
//         // println!("resp = {resp:?}");
//         // let resp = resp.error_for_status()?;
//         // let data = resp.bytes().await;
//         // println!("data = {data:?}");
//         // let data = resp.json::<NextProverJobPayload>().await?;
//         // let json_data = resp.json::<Option<NextProverJobPayload>>().await?;
//         // anyhow::bail!(None)
//         // Ok(json_data)
//         Ok(self
//             .http_client
//             .post(&url)
//             .send()
//             .await?
//             .error_for_status()?
//             .json::<Option<NextProverJobPayload>>()
//             .await?)
//     }
//
//     pub async fn submit_snark_proof(
//         &self,
//         proof: SnarkWrapperProof,
//         block_number: L1BatchNumber,
//     ) -> anyhow::Result<()> {
//         let url = format!("{}/prover-jobs/SNARK/submit", self.sequencer_url);
//         let payload = ProofPayload {
//             block_number: block_number.0,
//             proof: base64::encode(bincode::serialize(&proof)?),
//         };
//         self.http_client
//             .post(&url)
//             .json(&payload)
//             .send()
//             .await?
//             .error_for_status()?;
//         Ok(())
//     }
// }
//
pub struct SnarkJobPicker {
    sequencer_client: SequencerProofClient,
}

impl SnarkJobPicker {
    pub fn new(sequencer_url: String) -> Self {
        Self {
            sequencer_client: SequencerProofClient::new(sequencer_url),
        }
    }
}

fn deserialize_from_file<T: serde::de::DeserializeOwned>(filename: &str) -> T {
    let src = std::fs::File::open(filename).unwrap();
    serde_json::from_reader(src).unwrap()
}

#[async_trait]
impl JobPicker for SnarkJobPicker {
    type ExecutorType = SnarkExecutor;

    async fn pick_job(&mut self) -> anyhow::Result<Option<(ProgramProof, L2BlockNumber)>> {
        // let proof: ProgramProof = deserialize_from_file("/home/evl/anton_three_fri");
        let start_time = Instant::now();
        //
        // tracing::info!("Started picking snark job...");
        println!("Started picking snark job...");

        match self.sequencer_client.pick_snark_job().await? {
            None => {
                // tracing::info!("Finished picking snark job, got None from sequencer");
                println!("Finished picking snark job, got None from sequencer");
                Ok(None)
            }
            Some((proof, block)) => {
                // tracing::info!(
                //     "Finished picking snark job for {:?} in {:?}",
                //     &next_job.block_number,
                //     start_time.elapsed()
                // );
                println!(
                    "Finished picking snark job for {:?} in {:?}",
                    &block,
                    start_time.elapsed()
                );
                Ok(Some((proof, block)))
            }
        }
        // Ok(Some((proof, L1BatchNumber(1))))
    }
}
