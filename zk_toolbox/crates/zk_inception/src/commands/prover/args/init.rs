use clap::{Parser, ValueEnum};
use common::{Prompt, PromptSelect};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::messages::{
    MSG_PROOF_STORE_CONFIG_PROMPT, MSG_PROOF_STORE_DIR_PROMPT,
    MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT, MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT,
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct ProverInitArgs {
    #[clap(long)]
    pub proof_store_dir: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub proof_store_gcs_config: ProofStoreGCSConfig,
}

#[derive(Debug, Clone, ValueEnum, EnumIter, strum_macros::Display, PartialEq, Eq)]
enum ProofStoreConfig {
    Local,
    GCS,
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser, Default)]
pub struct ProofStoreGCSConfig {
    #[clap(long)]
    pub bucket_base_url: Option<String>,
    #[clap(long)]
    pub credentials_file: Option<String>,
}

impl ProverInitArgs {
    pub(crate) fn fill_values_with_prompt(&self) -> Self {
        if self.store_config_provided() {
            return self.clone();
        }

        let proof_store_config = if self.partial_gcs_config_provided() {
            ProofStoreConfig::GCS
        } else {
            PromptSelect::new(MSG_PROOF_STORE_CONFIG_PROMPT, ProofStoreConfig::iter()).ask()
        };

        match proof_store_config {
            ProofStoreConfig::Local => {
                let proof_store_dir = Prompt::new(MSG_PROOF_STORE_DIR_PROMPT).ask();

                ProverInitArgs {
                    proof_store_dir: Some(proof_store_dir),
                    proof_store_gcs_config: ProofStoreGCSConfig::default(),
                }
            }
            ProofStoreConfig::GCS => {
                let bucket_base_url = self
                    .clone()
                    .proof_store_gcs_config
                    .bucket_base_url
                    .unwrap_or_else(|| {
                        Prompt::new(MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT).ask()
                    });
                let credentials_file = self
                    .clone()
                    .proof_store_gcs_config
                    .credentials_file
                    .unwrap_or_else(|| {
                        Prompt::new(MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT).ask()
                    });
                let proof_store_gcs_config = ProofStoreGCSConfig {
                    bucket_base_url: Some(bucket_base_url),
                    credentials_file: Some(credentials_file),
                };

                ProverInitArgs {
                    proof_store_dir: None,
                    proof_store_gcs_config,
                }
            }
        }
    }

    fn store_config_provided(&self) -> bool {
        self.proof_store_dir.is_some()
            || (self.proof_store_gcs_config.bucket_base_url.is_some()
                && self.proof_store_gcs_config.credentials_file.is_some())
    }

    fn partial_gcs_config_provided(&self) -> bool {
        self.proof_store_gcs_config.bucket_base_url.is_some()
            || self.proof_store_gcs_config.credentials_file.is_some()
    }
}
