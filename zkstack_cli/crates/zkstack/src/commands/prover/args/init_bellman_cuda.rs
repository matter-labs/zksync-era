use clap::Parser;
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};
use zkstack_cli_common::{Prompt, PromptSelect};

use crate::messages::{
    MSG_BELLMAN_CUDA_DIR_PROMPT, MSG_BELLMAN_CUDA_ORIGIN_SELECT, MSG_BELLMAN_CUDA_SELECTION_CLONE,
    MSG_BELLMAN_CUDA_SELECTION_PATH,
};

#[derive(Debug, Clone, Parser, Default, Serialize, Deserialize)]
pub struct InitBellmanCudaArgs {
    #[clap(long, conflicts_with_all(["bellman_cuda_dir"]))]
    pub clone: bool,
    #[clap(long, conflicts_with_all(["clone"]))]
    pub bellman_cuda_dir: Option<String>,
}

#[derive(Debug, Clone, EnumIter, PartialEq, Eq)]
enum BellmanCudaPathSelection {
    Clone,
    Path,
}

impl std::fmt::Display for BellmanCudaPathSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BellmanCudaPathSelection::Clone => write!(f, "{MSG_BELLMAN_CUDA_SELECTION_CLONE}"),
            BellmanCudaPathSelection::Path => write!(f, "{MSG_BELLMAN_CUDA_SELECTION_PATH}"),
        }
    }
}

impl InitBellmanCudaArgs {
    pub fn fill_values_with_prompt(self) -> InitBellmanCudaArgs {
        let bellman_cuda_dir = if self.clone {
            "".to_string()
        } else {
            self.bellman_cuda_dir.unwrap_or_else(|| {
                match PromptSelect::new(
                    MSG_BELLMAN_CUDA_ORIGIN_SELECT,
                    BellmanCudaPathSelection::iter(),
                )
                .ask()
                {
                    BellmanCudaPathSelection::Clone => "".to_string(),
                    BellmanCudaPathSelection::Path => {
                        Prompt::new(MSG_BELLMAN_CUDA_DIR_PROMPT).ask()
                    }
                }
            })
        };

        InitBellmanCudaArgs {
            clone: self.clone,
            bellman_cuda_dir: Some(bellman_cuda_dir),
        }
    }
}
