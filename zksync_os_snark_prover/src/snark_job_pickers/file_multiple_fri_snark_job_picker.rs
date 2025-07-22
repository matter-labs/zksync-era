use crate::{
    client::SequencerProofClient,
    single_fri_snark_executor::{SingleFriSnarkExecutor, SingleFriSnarkExecutorMetadata},
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Instant;
use zksync_airbender_execution_utils::ProgramProof;
use zksync_prover_job_processor::JobPicker;

pub struct FileMultipleFriSnarkJobPicker {
    jobs: Vec<PathBuf>,
}

impl FileMultipleFriSnarkJobPicker {
    pub fn new(file_paths: Vec<String>) -> Self {
        let jobs = file_paths.iter().filter_map(|p| {
            let path = PathBuf::from(p);
            if path.exists() {
                Some(path)
            } else {
                tracing::warn!("File path does not exist: {}", p);
                None
            }
        }).collect();
        Self {
            jobs
        }
    }
}
