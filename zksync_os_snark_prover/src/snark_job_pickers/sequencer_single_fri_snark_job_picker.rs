use crate::{
    client::SequencerProofClient,
    single_fri_snark_executor::{SingleFriSnarkExecutor, SingleFriSnarkExecutorMetadata},
};
use async_trait::async_trait;
use std::time::Instant;
use zksync_airbender_execution_utils::ProgramProof;
use zksync_prover_job_processor::JobPicker;

pub struct SequencerSingleFriSnarkJobPicker {
    sequencer_client: SequencerProofClient,
}

impl SequencerSingleFriSnarkJobPicker {
    pub fn new(sequencer_url: String) -> Self {
        Self {
            sequencer_client: SequencerProofClient::new(sequencer_url),
        }
    }
}

#[async_trait]
impl JobPicker for SequencerSingleFriSnarkJobPicker {
    type ExecutorType = SingleFriSnarkExecutor;

    async fn pick_job(&mut self) -> anyhow::Result<Option<(ProgramProof, SingleFriSnarkExecutorMetadata)>> {
        let start_time = Instant::now();
        tracing::info!("Started picking snark job...");

        match self.sequencer_client.pick_snark_job().await? {
            None => {
                tracing::info!("Finished picking snark job, got None from sequencer");
                Ok(None)
            }
            Some((proof, block)) => {
                tracing::info!(
                    "Finished picking snark job for {:?} in {:?}",
                    &block,
                    start_time.elapsed()
                );
                Ok(Some((
                    proof,
                    SingleFriSnarkExecutorMetadata {
                        l2_block_number: block,
                    },
                )))
            }
        }
    }
}
