use crate::{
    client::SequencerProofClient,
    snark_executor::{SnarkExecutor, SnarkExecutorMetadata},
};
use async_trait::async_trait;
use std::time::Instant;
use zksync_airbender_execution_utils::ProgramProof;
use zksync_prover_job_processor::JobPicker;

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

#[async_trait]
impl JobPicker for SnarkJobPicker {
    type ExecutorType = SnarkExecutor;

    async fn pick_job(&mut self) -> anyhow::Result<Option<(ProgramProof, SnarkExecutorMetadata)>> {
        let start_time = Instant::now();
        println!("Started picking snark job...");

        match self.sequencer_client.pick_snark_job().await? {
            None => {
                println!("Finished picking snark job, got None from sequencer");
                Ok(None)
            }
            Some((proof, block)) => {
                println!(
                    "Finished picking snark job for {:?} in {:?}",
                    &block,
                    start_time.elapsed()
                );
                Ok(Some((
                    proof,
                    SnarkExecutorMetadata {
                        l2_block_number: block,
                    },
                )))
            }
        }
    }
}
