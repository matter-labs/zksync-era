use clap::{Parser, ValueEnum};
use common::{db::DatabaseConfig, logger, Prompt, PromptConfirm, PromptSelect};
use config::ChainConfig;
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use strum::{EnumIter, IntoEnumIterator};
use url::Url;
use xshell::Shell;
use zksync_config::configs::fri_prover::CloudConnectionMode;

use super::init_bellman_cuda::InitBellmanCudaArgs;
use crate::{
    commands::prover::gcs::get_project_ids,
    consts::{DEFAULT_CREDENTIALS_FILE, DEFAULT_PROOF_STORE_DIR},
    defaults::{generate_db_names, DBNames, DATABASE_PROVER_URL},
    messages::{
        msg_prover_db_name_prompt, msg_prover_db_url_prompt, MSG_CLOUD_TYPE_PROMPT,
        MSG_COPY_CONFIGS_PROMPT, MSG_CREATE_GCS_BUCKET_LOCATION_PROMPT,
        MSG_CREATE_GCS_BUCKET_NAME_PROMTP, MSG_CREATE_GCS_BUCKET_PROJECT_ID_NO_PROJECTS_PROMPT,
        MSG_CREATE_GCS_BUCKET_PROJECT_ID_PROMPT, MSG_CREATE_GCS_BUCKET_PROMPT,
        MSG_DOWNLOAD_SETUP_KEY_PROMPT, MSG_GETTING_PROOF_STORE_CONFIG,
        MSG_GETTING_PUBLIC_STORE_CONFIG, MSG_PROOF_STORE_CONFIG_PROMPT, MSG_PROOF_STORE_DIR_PROMPT,
        MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_ERR, MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT,
        MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT, MSG_SAVE_TO_PUBLIC_BUCKET_PROMPT,
        MSG_SETUP_KEY_PATH_PROMPT, MSG_USE_DEFAULT_DATABASES_HELP,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct ProverInitArgs {
    // Proof store object
    #[clap(long)]
    pub proof_store_dir: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub proof_store_gcs_config: ProofStorageGCSTmp,
    #[clap(flatten)]
    #[serde(flatten)]
    pub create_gcs_bucket_config: ProofStorageGCSCreateBucketTmp,

    // Public store object
    #[clap(long)]
    pub shall_save_to_public_bucket: Option<bool>,
    #[clap(long)]
    pub public_store_dir: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub public_store_gcs_config: PublicStorageGCSTmp,
    #[clap(flatten)]
    #[serde(flatten)]
    pub public_create_gcs_bucket_config: PublicStorageGCSCreateBucketTmp,

    // Bellman cuda
    #[clap(flatten)]
    #[serde(flatten)]
    pub bellman_cuda_config: InitBellmanCudaArgs,

    #[clap(flatten)]
    #[serde(flatten)]
    pub setup_key_config: SetupKeyConfigTmp,

    #[clap(long)]
    pub copy_configs: Option<bool>,

    #[clap(long)]
    pub setup_database: Option<bool>,
    #[clap(long, help = MSG_PROVER_DB_URL_HELP)]
    pub prover_db_url: Option<Url>,
    #[clap(long, help = MSG_PROVER_DB_NAME_HELP)]
    pub prover_db_name: Option<String>,
    #[clap(long, short, help = MSG_USE_DEFAULT_DATABASES_HELP)]
    pub use_default: bool,
    #[clap(long, short, action)]
    pub dont_drop: bool,

    #[clap(long)]
    cloud_type: Option<InternalCloudConnectionMode>,
}

#[derive(Debug, Clone, ValueEnum, EnumIter, strum::Display, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
enum ProofStoreConfig {
    Local,
    GCS,
}

#[derive(
    Debug, Clone, ValueEnum, EnumIter, strum::Display, PartialEq, Eq, Deserialize, Serialize,
)]
#[allow(clippy::upper_case_acronyms)]
enum InternalCloudConnectionMode {
    GCP,
    Local,
}

impl From<InternalCloudConnectionMode> for CloudConnectionMode {
    fn from(cloud_type: InternalCloudConnectionMode) -> Self {
        match cloud_type {
            InternalCloudConnectionMode::GCP => CloudConnectionMode::GCP,
            InternalCloudConnectionMode::Local => CloudConnectionMode::Local,
        }
    }
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
pub struct PublicStorageGCSTmp {
    #[clap(long)]
    pub public_bucket_base_url: Option<String>,
    #[clap(long)]
    pub public_credentials_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct PublicStorageGCSCreateBucketTmp {
    #[clap(long)]
    pub public_bucket_name: Option<String>,
    #[clap(long)]
    pub public_location: Option<String>,
    #[clap(long)]
    pub public_project_id: Option<String>,
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
#[allow(clippy::upper_case_acronyms)]
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
pub struct ProverDatabaseConfig {
    pub database_config: DatabaseConfig,
    pub dont_drop: bool,
}

#[derive(Debug, Clone)]
pub struct ProverInitArgsFinal {
    pub proof_store: ProofStorageConfig,
    pub public_store: Option<ProofStorageConfig>,
    pub setup_key_config: SetupKeyConfig,
    pub bellman_cuda_config: InitBellmanCudaArgs,
    pub cloud_type: CloudConnectionMode,
    pub copy_configs: bool,
    pub database_config: Option<ProverDatabaseConfig>,
}

impl ProverInitArgs {
    pub(crate) fn fill_values_with_prompt(
        &self,
        shell: &Shell,
        setup_key_path: &str,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<ProverInitArgsFinal> {
        let proof_store = self.fill_proof_storage_values_with_prompt(shell)?;
        let public_store = self.fill_public_storage_values_with_prompt(shell)?;
        let setup_key_config = self.fill_setup_key_values_with_prompt(setup_key_path);
        let bellman_cuda_config = self.fill_bellman_cuda_values_with_prompt()?;
        let cloud_type = self.get_cloud_type_with_prompt();
        let copy_configs = self.ask_copy_configs();
        let database_config = self.fill_database_values_with_prompt(chain_config);

        Ok(ProverInitArgsFinal {
            proof_store,
            public_store,
            setup_key_config,
            bellman_cuda_config,
            cloud_type,
            copy_configs,
            database_config,
        })
    }

    fn fill_proof_storage_values_with_prompt(
        &self,
        shell: &Shell,
    ) -> anyhow::Result<ProofStorageConfig> {
        logger::info(MSG_GETTING_PROOF_STORE_CONFIG);

        if self.proof_store_dir.is_some() {
            return Ok(self.handle_file_backed_config(self.proof_store_dir.clone()));
        }

        if self.partial_gcs_config_provided(
            self.proof_store_gcs_config.bucket_base_url.clone(),
            self.proof_store_gcs_config.credentials_file.clone(),
        ) {
            return Ok(self.ask_gcs_config(
                self.proof_store_gcs_config.bucket_base_url.clone(),
                self.proof_store_gcs_config.credentials_file.clone(),
            ));
        }

        if self.partial_create_gcs_bucket_config_provided(
            self.create_gcs_bucket_config.bucket_name.clone(),
            self.create_gcs_bucket_config.location.clone(),
            self.create_gcs_bucket_config.project_id.clone(),
        ) {
            let project_ids = get_project_ids(shell)?;
            return Ok(self.handle_create_gcs_bucket(
                project_ids,
                self.create_gcs_bucket_config.project_id.clone(),
                self.create_gcs_bucket_config.bucket_name.clone(),
                self.create_gcs_bucket_config.location.clone(),
                self.proof_store_gcs_config.credentials_file.clone(),
            ));
        }

        match PromptSelect::new(MSG_PROOF_STORE_CONFIG_PROMPT, ProofStoreConfig::iter()).ask() {
            ProofStoreConfig::Local => {
                Ok(self.handle_file_backed_config(self.proof_store_dir.clone()))
            }
            ProofStoreConfig::GCS => {
                let project_ids = get_project_ids(shell)?;
                Ok(self.handle_gcs_config(
                    project_ids,
                    self.proof_store_gcs_config.bucket_base_url.clone(),
                    self.proof_store_gcs_config.credentials_file.clone(),
                ))
            }
        }
    }

    fn fill_public_storage_values_with_prompt(
        &self,
        shell: &Shell,
    ) -> anyhow::Result<Option<ProofStorageConfig>> {
        logger::info(MSG_GETTING_PUBLIC_STORE_CONFIG);
        let shall_save_to_public_bucket = self
            .shall_save_to_public_bucket
            .unwrap_or_else(|| PromptConfirm::new(MSG_SAVE_TO_PUBLIC_BUCKET_PROMPT).ask());

        if !shall_save_to_public_bucket {
            return Ok(None);
        }

        if self.public_store_dir.is_some() {
            return Ok(Some(
                self.handle_file_backed_config(self.public_store_dir.clone()),
            ));
        }

        if self.partial_gcs_config_provided(
            self.public_store_gcs_config.public_bucket_base_url.clone(),
            self.public_store_gcs_config.public_credentials_file.clone(),
        ) {
            return Ok(Some(self.ask_gcs_config(
                self.public_store_gcs_config.public_bucket_base_url.clone(),
                self.public_store_gcs_config.public_credentials_file.clone(),
            )));
        }

        if self.partial_create_gcs_bucket_config_provided(
            self.public_create_gcs_bucket_config
                .public_bucket_name
                .clone(),
            self.public_create_gcs_bucket_config.public_location.clone(),
            self.public_create_gcs_bucket_config
                .public_project_id
                .clone(),
        ) {
            let project_ids = get_project_ids(shell)?;
            return Ok(Some(
                self.handle_create_gcs_bucket(
                    project_ids,
                    self.public_create_gcs_bucket_config
                        .public_project_id
                        .clone(),
                    self.public_create_gcs_bucket_config
                        .public_bucket_name
                        .clone(),
                    self.public_create_gcs_bucket_config.public_location.clone(),
                    self.public_store_gcs_config.public_credentials_file.clone(),
                ),
            ));
        }

        match PromptSelect::new(MSG_PROOF_STORE_CONFIG_PROMPT, ProofStoreConfig::iter()).ask() {
            ProofStoreConfig::Local => Ok(Some(
                self.handle_file_backed_config(self.public_store_dir.clone()),
            )),
            ProofStoreConfig::GCS => {
                let project_ids = get_project_ids(shell)?;
                Ok(Some(self.handle_gcs_config(
                    project_ids,
                    self.public_store_gcs_config.public_bucket_base_url.clone(),
                    self.public_store_gcs_config.public_credentials_file.clone(),
                )))
            }
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

    fn partial_create_gcs_bucket_config_provided(
        &self,
        bucket_name: Option<String>,
        location: Option<String>,
        project_id: Option<String>,
    ) -> bool {
        bucket_name.is_some() || location.is_some() || project_id.is_some()
    }

    fn partial_gcs_config_provided(
        &self,
        bucket_base_url: Option<String>,
        credentials_file: Option<String>,
    ) -> bool {
        bucket_base_url.is_some() || credentials_file.is_some()
    }

    fn handle_file_backed_config(&self, store_dir: Option<String>) -> ProofStorageConfig {
        let proof_store_dir = store_dir.unwrap_or_else(|| {
            Prompt::new(MSG_PROOF_STORE_DIR_PROMPT)
                .default(DEFAULT_PROOF_STORE_DIR)
                .ask()
        });

        ProofStorageConfig::FileBacked(ProofStorageFileBacked { proof_store_dir })
    }

    fn handle_gcs_config(
        &self,
        project_ids: Vec<String>,
        bucket_base_url: Option<String>,
        credentials_file: Option<String>,
    ) -> ProofStorageConfig {
        if !self.partial_gcs_config_provided(bucket_base_url.clone(), credentials_file.clone())
            && PromptConfirm::new(MSG_CREATE_GCS_BUCKET_PROMPT).ask()
        {
            return self.handle_create_gcs_bucket(project_ids, None, None, None, None);
        }

        self.ask_gcs_config(bucket_base_url, credentials_file)
    }

    fn handle_create_gcs_bucket(
        &self,
        project_ids: Vec<String>,
        project_id: Option<String>,
        bucket_name: Option<String>,
        location: Option<String>,
        credentials_file: Option<String>,
    ) -> ProofStorageConfig {
        let project_id = project_id.unwrap_or_else(|| {
            if project_ids.is_empty() {
                Prompt::new(MSG_CREATE_GCS_BUCKET_PROJECT_ID_NO_PROJECTS_PROMPT).ask()
            } else {
                PromptSelect::new(MSG_CREATE_GCS_BUCKET_PROJECT_ID_PROMPT, project_ids).ask()
            }
        });
        let bucket_name =
            bucket_name.unwrap_or_else(|| Prompt::new(MSG_CREATE_GCS_BUCKET_NAME_PROMTP).ask());
        let location =
            location.unwrap_or_else(|| Prompt::new(MSG_CREATE_GCS_BUCKET_LOCATION_PROMPT).ask());
        let credentials_file = credentials_file.unwrap_or_else(|| {
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

    fn ask_gcs_config(
        &self,
        bucket_base_url: Option<String>,
        credentials_file: Option<String>,
    ) -> ProofStorageConfig {
        let mut bucket_base_url = bucket_base_url
            .unwrap_or_else(|| Prompt::new(MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT).ask());
        while !bucket_base_url.starts_with("gs://") {
            logger::error(MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_ERR);
            bucket_base_url = Prompt::new(MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT).ask();
        }
        let credentials_file = credentials_file.unwrap_or_else(|| {
            Prompt::new(MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT)
                .default(DEFAULT_CREDENTIALS_FILE)
                .ask()
        });

        ProofStorageConfig::GCS(ProofStorageGCS {
            bucket_base_url,
            credentials_file,
        })
    }

    fn fill_bellman_cuda_values_with_prompt(&self) -> anyhow::Result<InitBellmanCudaArgs> {
        self.bellman_cuda_config.clone().fill_values_with_prompt()
    }

    fn get_cloud_type_with_prompt(&self) -> CloudConnectionMode {
        let cloud_type = self.cloud_type.clone().unwrap_or_else(|| {
            PromptSelect::new(MSG_CLOUD_TYPE_PROMPT, InternalCloudConnectionMode::iter()).ask()
        });

        cloud_type.into()
    }

    fn ask_copy_configs(&self) -> bool {
        self.copy_configs
            .unwrap_or_else(|| PromptConfirm::new(MSG_COPY_CONFIGS_PROMPT).ask())
    }

    fn fill_database_values_with_prompt(
        &self,
        config: &ChainConfig,
    ) -> Option<ProverDatabaseConfig> {
        let setup_database = self
            .setup_database
            .clone()
            .unwrap_or_else(|| PromptConfirm::new("Do you want to setup the database?").ask());

        if setup_database {
            let DBNames { prover_name, .. } = generate_db_names(config);
            let chain_name = config.name.clone();

            let dont_drop = self
                .dont_drop
                .clone()
                .unwrap_or_else(|| !PromptConfirm::new("Do you want to drop the database?").ask());

            if self
                .use_default
                .clone()
                .unwrap_or_else(|| PromptConfirm::new(MSG_USE_DEFAULT_DATABASES_HELP).ask())
            {
                Some(ProverDatabaseConfig {
                    database_config: DatabaseConfig::new(DATABASE_PROVER_URL.clone(), prover_name),
                    dont_drop,
                })
            } else {
                let prover_db_url = self.prover_db_url.clone().unwrap_or_else(|| {
                    Prompt::new(&msg_prover_db_url_prompt(&chain_name))
                        .default(DATABASE_PROVER_URL.as_str())
                        .ask()
                });

                let prover_db_name: String = self.prover_db_name.clone().unwrap_or_else(|| {
                    Prompt::new(&msg_prover_db_name_prompt(&chain_name))
                        .default(&prover_name)
                        .ask()
                });

                let prover_db_name = slugify!(&prover_db_name, separator = "_");

                Some(ProverDatabaseConfig {
                    database_config: DatabaseConfig::new(prover_db_url, prover_db_name),
                    dont_drop,
                })
            }
        } else {
            None
        }
    }
}
