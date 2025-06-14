use async_trait::async_trait;
use zkos_wrapper::SnarkWrapperProof;
use zksync_prover_job_processor::JobSaver;
use zksync_sequencer_proof_client::SequencerProofClient;
use zksync_types::L2BlockNumber;

use crate::snark_executor::SnarkExecutor;

pub struct SnarkJobSaver {
    sequencer_client: SequencerProofClient,
}

impl SnarkJobSaver {
    pub fn new(sequencer_url: String) -> Self {
        Self {
            sequencer_client: SequencerProofClient::new(sequencer_url),
        }
    }
}

#[async_trait]
impl JobSaver for SnarkJobSaver {
    type ExecutorType = SnarkExecutor;

    async fn save_job_result(
        &self,
        data: (anyhow::Result<SnarkWrapperProof>, L2BlockNumber),
    ) -> anyhow::Result<()> {
        // serialize_to_file(&data.0?, Path::new("/home/evl/snark_wrapper"));
        // Ok(())
        let (result, metadata) = data;
        match result {
            Ok(proof) => {
                self.sequencer_client
                    .submit_snark_proof(metadata, proof)
                    .await
            }
            Err(err) => {
                // tracing::error!("Failed to save SNARK job result: {:?}", err);
                println!("Error: Failed to save SNARK job result: {:?}", err);
                Err(err)
            }
        }
    }
}
