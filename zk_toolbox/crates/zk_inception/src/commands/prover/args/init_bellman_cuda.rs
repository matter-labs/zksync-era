use std::path::PathBuf;

use clap::Parser;
use common::Prompt;

use crate::messages::MSG_BELLMAN_CUDA_DIR_PROMPT;

#[derive(Debug, Clone, Parser, Default)]
pub struct InitBellmanCudaArgs {
    #[clap(long)]
    pub bellman_cuda_dir: Option<String>,
}

impl InitBellmanCudaArgs {
    pub fn fill_values_with_prompt(
        self,
        default_bellman_cuda_dir: &PathBuf,
    ) -> anyhow::Result<InitBellmanCudaArgs> {
        let bellman_cuda_dir = self.bellman_cuda_dir.unwrap_or_else(|| {
            Prompt::new(MSG_BELLMAN_CUDA_DIR_PROMPT)
                .default(default_bellman_cuda_dir.to_str().unwrap_or_default())
                .ask()
        });

        Ok(InitBellmanCudaArgs {
            bellman_cuda_dir: Some(bellman_cuda_dir),
        })
    }
}
