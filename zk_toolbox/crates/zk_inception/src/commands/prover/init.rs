use std::path::PathBuf;

use anyhow::Context;
use common::{cmd::Cmd, logger, spinner::Spinner};
use config::{
    copy_prover_configs, traits::SaveConfigWithBasePath, EcosystemConfig, GeneralProverConfig,
    ProverConfig,
};
use xshell::{cmd, Shell};
use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};

use super::{
    args::init::{ProofStorageConfig, ProverInitArgs},
    gcs::create_gcs_bucket,
    init_bellman_cuda::run as init_bellman_cuda,
    utils::get_link_to_prover,
};
use crate::{
    consts::PROVER_STORE_MAX_RETRIES,
    messages::{
        MSG_CHAIN_NOT_FOUND_ERR, MSG_CONFIGS_NOT_FOUND_ERR, MSG_DOWNLOADING_SETUP_KEY_SPINNER,
        MSG_GENERAL_CONFIG_NOT_FOUND_ERR, MSG_PROVER_INITIALIZED, MSG_SETUP_KEY_PATH_ERROR,
    },
};

pub(crate) async fn run(args: ProverInitArgs, shell: &Shell) -> anyhow::Result<()> {
    //todo: uncomment check_prover_prequisites(shell);

    let current_path = shell.current_dir();

    let prover_only_mode = match EcosystemConfig::from_file(shell) {
        Ok(_) => false,
        Err(_) => {
            let _dir = shell.push_dir(current_path.clone());
            match GeneralProverConfig::from_file(shell) {
                Ok(_) => true,
                Err(_) => {
                    return Err(anyhow::anyhow!(MSG_CONFIGS_NOT_FOUND_ERR));
                }
            }
        }
    };

    let (link_to_code, configs) = if prover_only_mode {
        let general_prover_config = GeneralProverConfig::from_file(shell)?;
        (
            general_prover_config.link_to_code,
            general_prover_config.config,
        )
    } else {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        (ecosystem_config.link_to_code, ecosystem_config.config)
    };

    if prover_only_mode {
        copy_prover_configs(shell, &link_to_code, &configs)?;
    }

    let mut prover_config = if prover_only_mode {
        let prover_config = GeneralProverConfig::from_file(shell)?;
        prover_config.load_prover_config()?
    } else {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_config = ecosystem_config
            .load_chain(Some(ecosystem_config.default_chain.clone()))
            .context(MSG_CHAIN_NOT_FOUND_ERR)?;
        let general_config = chain_config
            .get_general_config()
            .context(MSG_GENERAL_CONFIG_NOT_FOUND_ERR)?;
        ProverConfig::from(general_config)
    };

    let setup_key_path = get_default_setup_key_path(link_to_code.clone())?;

    let args = args.fill_values_with_prompt(shell, &setup_key_path)?;

    let proof_object_store_config = get_object_store_config(shell, Some(args.proof_store))?;
    let public_object_store_config = get_object_store_config(shell, args.public_store)?;

    if args.setup_key_config.download_key {
        download_setup_key(shell, &prover_config, &args.setup_key_config.setup_key_path)?;
    }

    prover_config
        .prover
        .prover_object_store
        .clone_from(&proof_object_store_config);
    if let Some(public_object_store_config) = public_object_store_config {
        prover_config.prover.shall_save_to_public_bucket = true;
        prover_config.prover.public_object_store = Some(public_object_store_config);
    } else {
        prover_config.prover.shall_save_to_public_bucket = false;
    }
    prover_config.prover.cloud_type = args.cloud_type;

    prover_config.proof_compressor.universal_setup_path = args.setup_key_config.setup_key_path;

    if prover_only_mode {
        prover_config.save_with_base_path(shell, &configs)?;
    } else {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_config = ecosystem_config
            .load_chain(Some(ecosystem_config.default_chain.clone()))
            .context(MSG_CHAIN_NOT_FOUND_ERR)?;
        let mut general_config = chain_config
            .get_general_config()
            .context(MSG_GENERAL_CONFIG_NOT_FOUND_ERR)?;

        general_config.prover_config = Some(prover_config.prover);
        general_config.proof_compressor_config = Some(prover_config.proof_compressor);

        chain_config.save_general_config(&general_config)?;
    }

    init_bellman_cuda(shell, args.bellman_cuda_config).await?;

    logger::outro(MSG_PROVER_INITIALIZED);
    Ok(())
}

fn download_setup_key(
    shell: &Shell,
    prover_config: &ProverConfig,
    path: &str,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_DOWNLOADING_SETUP_KEY_SPINNER);
    let url = prover_config
        .proof_compressor
        .universal_setup_download_url
        .clone();
    let path = std::path::Path::new(path);
    let parent = path.parent().expect(MSG_SETUP_KEY_PATH_ERROR);
    let file_name = path.file_name().expect(MSG_SETUP_KEY_PATH_ERROR);

    Cmd::new(cmd!(shell, "wget {url} -P {parent}")).run()?;

    if file_name != "setup_2^24.key" {
        Cmd::new(cmd!(shell, "mv {parent}/setup_2^24.key {path}")).run()?;
    }

    spinner.finish();
    Ok(())
}

fn get_default_setup_key_path(link_to_code: PathBuf) -> anyhow::Result<String> {
    let link_to_prover = get_link_to_prover(link_to_code);
    let path = link_to_prover.join("keys/setup/setup_2^24.key");
    let string = path.to_str().unwrap();

    Ok(String::from(string))
}

fn get_object_store_config(
    shell: &Shell,
    config: Option<ProofStorageConfig>,
) -> anyhow::Result<Option<ObjectStoreConfig>> {
    let object_store = match config {
        Some(ProofStorageConfig::FileBacked(config)) => Some(ObjectStoreConfig {
            mode: ObjectStoreMode::FileBacked {
                file_backed_base_path: config.proof_store_dir,
            },
            max_retries: PROVER_STORE_MAX_RETRIES,
            local_mirror_path: None,
        }),
        Some(ProofStorageConfig::GCS(config)) => Some(ObjectStoreConfig {
            mode: ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url: config.bucket_base_url,
                gcs_credential_file_path: config.credentials_file,
            },
            max_retries: PROVER_STORE_MAX_RETRIES,
            local_mirror_path: None,
        }),
        Some(ProofStorageConfig::GCSCreateBucket(config)) => {
            Some(create_gcs_bucket(shell, config)?)
        }
        None => None,
    };

    Ok(object_store)
}
