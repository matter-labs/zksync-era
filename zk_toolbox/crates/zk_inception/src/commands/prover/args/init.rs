use clap::{Parser, ValueEnum};
use common::{Prompt, PromptConfirm, PromptSelect};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::messages::{
    MSG_CREATE_GCS_BUCKET_LOCATION_PROMPT, MSG_CREATE_GCS_BUCKET_NAME_PROMTP,
    MSG_CREATE_GCS_BUCKET_PROJECT_ID_PROMPT, MSG_CREATE_GCS_BUCKET_PROMPT,
    MSG_PROOF_STORE_CONFIG_PROMPT, MSG_PROOF_STORE_DIR_PROMPT,
    MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT, MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT,
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct ProverInitArgs {
    #[clap(long)]
    pub proof_store_dir: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub proof_store_gcs_config: ProofStoreGCSConfig,
    #[clap(flatten)]
    #[serde(flatten)]
    pub create_gcs_bucket_config: CreateGCSBucketConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct CreateGCSBucketConfig {
    #[clap(long)]
    pub bucket_name: Option<String>,
    #[clap(long)]
    pub location: Option<String>,
    #[clap(long)]
    pub project_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProofStorageFileBacked {
    pub proof_store_dir: String,
}

#[derive(Debug, Clone)]
pub struct ProofStorageGCS {
    pub bucket_base_url: String,
    pub credentials_file: String,
}

#[derive(Debug, Clone)]
pub struct ProofStorageGCSCreateBucket {
    pub bucket_name: String,
    pub location: String,
    pub project_id: String,
    pub credentials_file: String,
}

#[derive(Debug, Clone)]
pub enum ProverInitArgsFinal {
    FileBacked(ProofStorageFileBacked),
    GCS(ProofStorageGCS),
    GCSCreateBucket(ProofStorageGCSCreateBucket),
}

impl ProverInitArgs {
    pub(crate) fn fill_values_with_prompt(&self) -> ProverInitArgsFinal {
        if self.proof_store_dir.is_some() {
            return self.handle_file_backed_config();
        }

        if self.partial_gcs_config_provided() {
            return self.ask_gcs_config();
        }

        if self.partial_create_gcs_bucket_config_provided() {
            return self.handle_create_gcs_bucket();
        }

        let proof_store_config =
            PromptSelect::new(MSG_PROOF_STORE_CONFIG_PROMPT, ProofStoreConfig::iter()).ask();

        match proof_store_config {
            ProofStoreConfig::Local => self.handle_file_backed_config(),
            ProofStoreConfig::GCS => self.handle_gcs_config(),
        }
    }

    fn partial_create_gcs_bucket_config_provided(&self) -> bool {
        self.create_gcs_bucket_config.bucket_name.is_some()
            || self.create_gcs_bucket_config.location.is_some()
    }

    fn partial_gcs_config_provided(&self) -> bool {
        self.proof_store_gcs_config.bucket_base_url.is_some()
            || self.proof_store_gcs_config.credentials_file.is_some()
    }

    fn handle_file_backed_config(&self) -> ProverInitArgsFinal {
        let proof_store_dir = self
            .proof_store_dir
            .clone()
            .unwrap_or_else(|| Prompt::new(MSG_PROOF_STORE_DIR_PROMPT).ask());

        ProverInitArgsFinal::FileBacked(ProofStorageFileBacked { proof_store_dir })
    }

    fn handle_gcs_config(&self) -> ProverInitArgsFinal {
        if !self.partial_gcs_config_provided() {
            if PromptConfirm::new(MSG_CREATE_GCS_BUCKET_PROMPT).ask() {
                return self.handle_create_gcs_bucket();
            }
        }

        self.ask_gcs_config()
    }

    fn handle_create_gcs_bucket(&self) -> ProverInitArgsFinal {
        let project_id = self
            .create_gcs_bucket_config
            .project_id
            .clone()
            .unwrap_or_else(|| Prompt::new(MSG_CREATE_GCS_BUCKET_PROJECT_ID_PROMPT).ask());
        let bucket_name = self
            .create_gcs_bucket_config
            .bucket_name
            .clone()
            .unwrap_or_else(|| Prompt::new(MSG_CREATE_GCS_BUCKET_NAME_PROMTP).ask());
        let location = self
            .create_gcs_bucket_config
            .location
            .clone()
            .unwrap_or_else(|| Prompt::new(MSG_CREATE_GCS_BUCKET_LOCATION_PROMPT).ask());
        let credentials_file = self
            .clone()
            .proof_store_gcs_config
            .credentials_file
            .unwrap_or_else(|| Prompt::new(MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT).ask());

        ProverInitArgsFinal::GCSCreateBucket(ProofStorageGCSCreateBucket {
            bucket_name,
            location,
            project_id,
            credentials_file,
        })
    }

    fn ask_gcs_config(&self) -> ProverInitArgsFinal {
        let bucket_base_url = self
            .clone()
            .proof_store_gcs_config
            .bucket_base_url
            .unwrap_or_else(|| Prompt::new(MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT).ask());
        let credentials_file = self
            .clone()
            .proof_store_gcs_config
            .credentials_file
            .unwrap_or_else(|| Prompt::new(MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT).ask());

        ProverInitArgsFinal::GCS(ProofStorageGCS {
            bucket_base_url,
            credentials_file,
        })
    }
}
