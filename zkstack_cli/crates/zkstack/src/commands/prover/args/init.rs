use std::path::Path;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use strum::{EnumIter, IntoEnumIterator};
use url::Url;
use xshell::Shell;
use zkstack_cli_common::{db::DatabaseConfig, logger, Prompt, PromptConfirm, PromptSelect};
use zkstack_cli_config::ChainConfig;

use super::{
    compressor_keys::CompressorKeysArgs, init_bellman_cuda::InitBellmanCudaArgs,
    setup_keys::SetupKeysArgs,
};
use crate::{
    commands::prover::gcs::get_project_ids,
    consts::{DEFAULT_CREDENTIALS_FILE, DEFAULT_PROOF_STORE_DIR},
    defaults::{generate_db_names, DBNames, DATABASE_PROVER_URL},
    messages::{
        msg_prover_db_name_prompt, msg_prover_db_url_prompt, MSG_CREATE_GCS_BUCKET_LOCATION_PROMPT,
        MSG_CREATE_GCS_BUCKET_NAME_PROMTP, MSG_CREATE_GCS_BUCKET_PROJECT_ID_NO_PROJECTS_PROMPT,
        MSG_CREATE_GCS_BUCKET_PROJECT_ID_PROMPT, MSG_CREATE_GCS_BUCKET_PROMPT,
        MSG_DOWNLOAD_SETUP_COMPRESSOR_KEY_PROMPT, MSG_GETTING_PROOF_STORE_CONFIG,
        MSG_INITIALIZE_BELLMAN_CUDA_PROMPT, MSG_PROOF_STORE_CONFIG_PROMPT,
        MSG_PROOF_STORE_DIR_PROMPT, MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_ERR,
        MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT, MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT,
        MSG_PROVER_DB_NAME_HELP, MSG_PROVER_DB_URL_HELP, MSG_SETUP_KEYS_PROMPT,
        MSG_USE_DEFAULT_DATABASES_HELP,
    },
};

#[derive(Debug, Clone, Parser, Default)]
pub struct ProverInitArgs {
    #[clap(long)]
    pub dev: bool,

    // Proof store object
    #[clap(long)]
    pub proof_store_dir: Option<String>,
    #[clap(flatten)]
    pub proof_store_gcs_config: ProofStorageGCSTmp,
    #[clap(flatten)]
    pub create_gcs_bucket_config: ProofStorageGCSCreateBucketTmp,

    #[clap(long, default_missing_value = "false", num_args = 0..=1)]
    pub deploy_proving_networks: Option<bool>,

    // Bellman cuda
    #[clap(flatten)]
    pub bellman_cuda_config: InitBellmanCudaArgs,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub bellman_cuda: Option<bool>,

    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub setup_compressor_key: Option<bool>,
    #[clap(flatten)]
    pub compressor_keys_args: CompressorKeysArgs,

    #[clap(flatten)]
    pub setup_keys_args: SetupKeysArgs,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub setup_keys: Option<bool>,

    #[clap(long)]
    pub setup_database: Option<bool>,
    #[clap(long, help = MSG_PROVER_DB_URL_HELP)]
    pub prover_db_url: Option<Url>,
    #[clap(long, help = MSG_PROVER_DB_NAME_HELP)]
    pub prover_db_name: Option<String>,
    #[clap(long, short, help = MSG_USE_DEFAULT_DATABASES_HELP)]
    pub use_default: Option<bool>,
    #[clap(long, short, action)]
    pub dont_drop: Option<bool>,
}

#[derive(Debug, Clone, ValueEnum, EnumIter, strum::Display, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
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
pub struct SetupCompressorKeyConfigTmp {
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
pub struct ProverDatabaseConfig {
    pub database_config: DatabaseConfig,
    pub dont_drop: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ProverInitArgsFinal {
    pub proof_store: ProofStorageConfig,
    pub compressor_key_args: Option<CompressorKeysArgs>,
    pub setup_keys: Option<SetupKeysArgs>,
    pub bellman_cuda_config: Option<InitBellmanCudaArgs>,
    pub database_config: Option<ProverDatabaseConfig>,
    pub deploy_proving_networks: bool,
}

impl ProverInitArgs {
    pub(crate) fn fill_values_with_prompt(
        &self,
        shell: &Shell,
        default_compressor_key_path: &Path,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<ProverInitArgsFinal> {
        let deploy_proving_networks = self.fill_deploy_proving_networks_values_with_prompt();
        let proof_store = self.fill_proof_storage_values_with_prompt(shell)?;
        let compressor_key_args =
            self.fill_setup_compressor_key_values_with_prompt(default_compressor_key_path);
        let bellman_cuda_config = self.fill_bellman_cuda_values_with_prompt();
        let database_config = self.fill_database_values_with_prompt(chain_config);
        let setup_keys = self.fill_setup_keys_values_with_prompt();

        Ok(ProverInitArgsFinal {
            proof_store,
            compressor_key_args,
            setup_keys,
            bellman_cuda_config,
            database_config,
            deploy_proving_networks,
        })
    }

    fn fill_deploy_proving_networks_values_with_prompt(&self) -> bool {
        self.deploy_proving_networks.unwrap_or_else(|| {
            PromptConfirm::new("Do you want to deploy proving networks?")
                .default(false)
                .ask()
        })
    }

    fn fill_proof_storage_values_with_prompt(
        &self,
        shell: &Shell,
    ) -> anyhow::Result<ProofStorageConfig> {
        logger::info(MSG_GETTING_PROOF_STORE_CONFIG);

        if self.dev {
            return Ok(self.handle_file_backed_config(Some(DEFAULT_PROOF_STORE_DIR.to_string())));
        }

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

    fn fill_setup_compressor_key_values_with_prompt(
        &self,
        default_path: &Path,
    ) -> Option<CompressorKeysArgs> {
        if self.dev {
            return Some(CompressorKeysArgs {
                path: Some(default_path.to_owned()),
            });
        }

        let download_key = self.clone().setup_compressor_key.unwrap_or_else(|| {
            PromptConfirm::new(MSG_DOWNLOAD_SETUP_COMPRESSOR_KEY_PROMPT)
                .default(false)
                .ask()
        });

        if download_key {
            Some(
                self.compressor_keys_args
                    .clone()
                    .fill_values_with_prompt(default_path),
            )
        } else {
            None
        }
    }

    fn fill_setup_keys_values_with_prompt(&self) -> Option<SetupKeysArgs> {
        if self.dev {
            return None;
        }
        let args = self.setup_keys_args.clone();

        if self.setup_keys.unwrap_or_else(|| {
            PromptConfirm::new(MSG_SETUP_KEYS_PROMPT)
                .default(false)
                .ask()
        }) {
            Some(args)
        } else {
            None
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

    fn fill_bellman_cuda_values_with_prompt(&self) -> Option<InitBellmanCudaArgs> {
        if self.dev {
            return None;
        }

        let args = self.bellman_cuda_config.clone();
        if self.bellman_cuda.unwrap_or_else(|| {
            PromptConfirm::new(MSG_INITIALIZE_BELLMAN_CUDA_PROMPT)
                .default(false)
                .ask()
        }) {
            Some(args)
        } else {
            None
        }
    }

    fn fill_database_values_with_prompt(
        &self,
        config: &ChainConfig,
    ) -> Option<ProverDatabaseConfig> {
        let setup_database = self.dev
            || self
                .setup_database
                .unwrap_or_else(|| PromptConfirm::new("Do you want to setup the database?").ask());

        if setup_database {
            let DBNames { prover_name, .. } = generate_db_names(config);
            let chain_name = config.name.clone();

            let dont_drop = if !self.dev {
                self.dont_drop.unwrap_or_else(|| {
                    !PromptConfirm::new("Do you want to drop the database?")
                        .default(true)
                        .ask()
                })
            } else {
                false
            };

            if self.dev
                || self.use_default.unwrap_or_else(|| {
                    PromptConfirm::new(MSG_USE_DEFAULT_DATABASES_HELP)
                        .default(true)
                        .ask()
                })
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
