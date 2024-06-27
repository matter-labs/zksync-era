use clap::{Parser, ValueEnum};
use common::{Prompt, PromptConfirm, PromptSelect};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::messages::{
    MSG_CREATE_GCS_BUCKET_LOCATION_PROMPT, MSG_CREATE_GCS_BUCKET_NAME_PROMTP,
    MSG_CREATE_GCS_BUCKET_PROJECT_ID_NO_PROJECTS_PROMPT, MSG_CREATE_GCS_BUCKET_PROJECT_ID_PROMPT,
    MSG_CREATE_GCS_BUCKET_PROMPT, MSG_DOWNLOAD_SETUP_KEY_PROMPT, MSG_PROOF_STORE_CONFIG_PROMPT,
    MSG_PROOF_STORE_DIR_PROMPT, MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT,
    MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT, MSG_SETUP_KEY_PATH_PROMPT,
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct ProverInitArgs {
    #[clap(long)]
    pub proof_store_dir: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub proof_store_gcs_config: ProofStorageGCSTmp,
    #[clap(flatten)]
    #[serde(flatten)]
    pub create_gcs_bucket_config: ProofStorageGCSCreateBucketTmp,
    #[clap(flatten)]
    #[serde(flatten)]
    pub setup_key_config: SetupKeyConfigTmp,
}

#[derive(Debug, Clone, ValueEnum, EnumIter, strum_macros::Display, PartialEq, Eq)]
enum ProofStoreConfig {
    Local,
    GCS,
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser, Default)]
pub struct ProofStorageGCSTmp {
    #[clap(long)]
    pub bucket_base_url: Option<String>,
    #[clap(long)]
    pub credentials_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct ProofStorageGCSCreateBucketTmp {
    #[clap(long)]
    pub bucket_name: Option<String>,
    #[clap(long)]
    pub location: Option<String>,
    #[clap(long)]
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser, Default)]
pub struct SetupKeyConfigTmp {
    #[clap(long)]
    pub download_key: Option<bool>,
    #[clap(long)]
    pub setup_key_path: Option<String>,
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
pub enum ProofStorageConfig {
    FileBacked(ProofStorageFileBacked),
    GCS(ProofStorageGCS),
    GCSCreateBucket(ProofStorageGCSCreateBucket),
}

#[derive(Debug, Clone)]
pub struct SetupKeyConfig {
    pub download_key: bool,
    pub setup_key_path: String,
}

#[derive(Debug, Clone)]
pub struct ProverInitArgsFinal {
    pub proof_store: ProofStorageConfig,
    pub setup_key_config: SetupKeyConfig,
}

const DEFAULT_CREDENTIALS_FILE: &str = "~/.config/gcloud/application_default_credentials.json";
const DEFAULT_PROOF_STORE_DIR: &str = "artifacts";

impl ProverInitArgs {
    pub(crate) fn fill_values_with_prompt(
        &self,
        project_ids: Vec<String>,
        setup_key_path: &str,
    ) -> ProverInitArgsFinal {
        let proof_store = self.fill_proof_storage_values_with_prompt(project_ids);
        let setup_key_config = self.fill_setup_key_values_with_prompt(setup_key_path);
        ProverInitArgsFinal {
            proof_store,
            setup_key_config,
        }
    }

    fn fill_proof_storage_values_with_prompt(
        &self,
        project_ids: Vec<String>,
    ) -> ProofStorageConfig {
        if self.proof_store_dir.is_some() {
            return self.handle_file_backed_config();
        }

        if self.partial_gcs_config_provided() {
            return self.ask_gcs_config();
        }

        if self.partial_create_gcs_bucket_config_provided() {
            return self.handle_create_gcs_bucket(project_ids);
        }

        let proof_store_config =
            PromptSelect::new(MSG_PROOF_STORE_CONFIG_PROMPT, ProofStoreConfig::iter()).ask();

        match proof_store_config {
            ProofStoreConfig::Local => self.handle_file_backed_config(),
            ProofStoreConfig::GCS => self.handle_gcs_config(project_ids),
        }
    }

    fn fill_setup_key_values_with_prompt(&self, setup_key_path: &str) -> SetupKeyConfig {
        let download_key = self
            .clone()
            .setup_key_config
            .download_key
            .unwrap_or_else(|| PromptConfirm::new(MSG_DOWNLOAD_SETUP_KEY_PROMPT).ask());
        let setup_key_path = self
            .clone()
            .setup_key_config
            .setup_key_path
            .unwrap_or_else(|| {
                Prompt::new(MSG_SETUP_KEY_PATH_PROMPT)
                    .default(setup_key_path)
                    .ask()
            });

        SetupKeyConfig {
            download_key,
            setup_key_path,
        }
    }

    fn partial_create_gcs_bucket_config_provided(&self) -> bool {
        self.create_gcs_bucket_config.bucket_name.is_some()
            || self.create_gcs_bucket_config.location.is_some()
            || self.create_gcs_bucket_config.project_id.is_some()
    }

    fn partial_gcs_config_provided(&self) -> bool {
        self.proof_store_gcs_config.bucket_base_url.is_some()
            || self.proof_store_gcs_config.credentials_file.is_some()
    }

    fn handle_file_backed_config(&self) -> ProofStorageConfig {
        let proof_store_dir = self.proof_store_dir.clone().unwrap_or_else(|| {
            Prompt::new(MSG_PROOF_STORE_DIR_PROMPT)
                .default(DEFAULT_PROOF_STORE_DIR)
                .ask()
        });

        ProofStorageConfig::FileBacked(ProofStorageFileBacked { proof_store_dir })
    }

    fn handle_gcs_config(&self, project_ids: Vec<String>) -> ProofStorageConfig {
        if !self.partial_gcs_config_provided() {
            if PromptConfirm::new(MSG_CREATE_GCS_BUCKET_PROMPT).ask() {
                return self.handle_create_gcs_bucket(project_ids);
            }
        }

        self.ask_gcs_config()
    }

    fn handle_create_gcs_bucket(&self, project_ids: Vec<String>) -> ProofStorageConfig {
        let project_id = self
            .create_gcs_bucket_config
            .project_id
            .clone()
            .unwrap_or_else(|| {
                if project_ids.is_empty() {
                    Prompt::new(MSG_CREATE_GCS_BUCKET_PROJECT_ID_NO_PROJECTS_PROMPT).ask()
                } else {
                    PromptSelect::new(MSG_CREATE_GCS_BUCKET_PROJECT_ID_PROMPT, project_ids).ask()
                }
            });
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
            .unwrap_or_else(|| {
                Prompt::new(MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT)
                    .default(DEFAULT_CREDENTIALS_FILE)
                    .ask()
            });

        ProofStorageConfig::GCSCreateBucket(ProofStorageGCSCreateBucket {
            bucket_name,
            location,
            project_id,
            credentials_file,
        })
    }

    fn ask_gcs_config(&self) -> ProofStorageConfig {
        let bucket_base_url = self
            .clone()
            .proof_store_gcs_config
            .bucket_base_url
            .unwrap_or_else(|| Prompt::new(MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT).ask());
        let credentials_file = self
            .clone()
            .proof_store_gcs_config
            .credentials_file
            .unwrap_or_else(|| {
                Prompt::new(MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT)
                    .default(DEFAULT_CREDENTIALS_FILE)
                    .ask()
            });

        ProofStorageConfig::GCS(ProofStorageGCS {
            bucket_base_url,
            credentials_file,
        })
    }
}
