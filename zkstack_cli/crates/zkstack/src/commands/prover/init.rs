use std::path::PathBuf;

use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{
    cmd::Cmd,
    config::global_config,
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    logger,
    spinner::Spinner,
};
use zkstack_cli_config::{
    copy_configs, get_link_to_prover, raw::PatchedConfig, set_prover_database, EcosystemConfig,
};
use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};

use super::{
    args::init::{ProofStorageConfig, ProofStorageFileBacked, ProverInitArgs},
    compressor_keys::{download_compressor_key, get_default_compressor_keys_path},
    gcs::create_gcs_bucket,
    init_bellman_cuda::run as init_bellman_cuda,
    setup_keys,
};
use crate::{
    consts::{PROVER_MIGRATIONS, PROVER_STORE_MAX_RETRIES},
    messages::{
        MSG_CHAIN_NOT_FOUND_ERR, MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR,
        MSG_INITIALIZING_DATABASES_SPINNER, MSG_INITIALIZING_PROVER_DATABASE,
        MSG_PROVER_INITIALIZED, MSG_SETUP_KEY_PATH_ERROR,
    },
};

pub(crate) async fn run(args: ProverInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let default_compressor_key_path = get_default_compressor_keys_path(&ecosystem_config)?;

    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let args = args.fill_values_with_prompt(shell, &default_compressor_key_path, &chain_config)?;

    if chain_config.get_general_config().await.is_err()
        || chain_config.get_secrets_config().await.is_err()
    {
        copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;
    }

    let mut general_config = chain_config.get_general_config().await?.patched();

    let proof_object_store_config =
        get_object_store_config(shell, Some(args.proof_store))?.unwrap();
    let public_object_store_config = get_object_store_config(shell, args.public_store)?;

    if let Some(args) = args.compressor_key_args {
        let path = args.path.context(MSG_SETUP_KEY_PATH_ERROR)?;

        download_compressor_key(shell, &mut general_config, &path)?;
    }

    if let Some(args) = args.setup_keys {
        setup_keys::run(args, shell).await?;
    }

    set_object_store(
        &mut general_config,
        "prover.prover_object_store",
        &proof_object_store_config,
    )?;
    if let Some(public_object_store_config) = public_object_store_config {
        general_config.insert("prover.shall_save_to_public_bucket", true)?;
        set_object_store(
            &mut general_config,
            "prover.public_object_store",
            &public_object_store_config,
        )?;
    } else {
        general_config.insert("prover.shall_save_to_public_bucket", false)?;
    }
    general_config.insert_yaml("prover.cloud_type", args.cloud_type)?;
    general_config.save().await?;

    if let Some(args) = args.bellman_cuda_config {
        init_bellman_cuda(shell, args).await?;
    }

    if let Some(prover_db) = &args.database_config {
        let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);

        let mut secrets = chain_config.get_secrets_config().await?.patched();
        set_prover_database(&mut secrets, &prover_db.database_config)?;
        secrets.save().await?;
        initialize_prover_database(
            shell,
            &prover_db.database_config,
            ecosystem_config.link_to_code.clone(),
            prover_db.dont_drop,
        )
        .await?;

        spinner.finish();
    }

    logger::outro(MSG_PROVER_INITIALIZED);
    Ok(())
}

fn get_object_store_config(
    shell: &Shell,
    config: Option<ProofStorageConfig>,
) -> anyhow::Result<Option<ObjectStoreConfig>> {
    let object_store = match config {
        Some(ProofStorageConfig::FileBacked(config)) => Some(init_file_backed_proof_storage(
            shell,
            &EcosystemConfig::from_file(shell)?,
            config,
        )?),
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

fn set_object_store(
    patch: &mut PatchedConfig,
    prefix: &str,
    config: &ObjectStoreConfig,
) -> anyhow::Result<()> {
    patch.insert(&format!("{prefix}.max_retries"), config.max_retries)?;
    match &config.mode {
        ObjectStoreMode::FileBacked {
            file_backed_base_path,
        } => {
            patch.insert_yaml(
                &format!("{prefix}.file_backed.file_backed_base_path"),
                file_backed_base_path,
            )?;
        }
        ObjectStoreMode::GCS { bucket_base_url } => {
            patch.insert(
                &format!("{prefix}.gcs.bucket_base_url"),
                bucket_base_url.clone(),
            )?;
        }
        ObjectStoreMode::GCSWithCredentialFile {
            bucket_base_url,
            gcs_credential_file_path,
        } => {
            patch.insert(
                &format!("{prefix}.gcs_with_credential_file.bucket_base_url"),
                bucket_base_url.clone(),
            )?;
            patch.insert(
                &format!("{prefix}.gcs_with_credential_file.gcs_credential_file_path"),
                gcs_credential_file_path.clone(),
            )?;
        }
        ObjectStoreMode::GCSAnonymousReadOnly { bucket_base_url } => {
            patch.insert(
                &format!("{prefix}.gcs_anonymous_read_only.bucket_base_url"),
                bucket_base_url.clone(),
            )?;
        }
    }
    Ok(())
}

async fn initialize_prover_database(
    shell: &Shell,
    prover_db_config: &DatabaseConfig,
    link_to_code: PathBuf,
    dont_drop: bool,
) -> anyhow::Result<()> {
    if global_config().verbose {
        logger::debug(MSG_INITIALIZING_PROVER_DATABASE)
    }
    if !dont_drop {
        drop_db_if_exists(prover_db_config)
            .await
            .context(MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR)?;
        init_db(prover_db_config).await?;
    }
    let path_to_prover_migration = link_to_code.join(PROVER_MIGRATIONS);
    migrate_db(
        shell,
        path_to_prover_migration,
        &prover_db_config.full_url(),
    )
    .await?;

    Ok(())
}

fn init_file_backed_proof_storage(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    config: ProofStorageFileBacked,
) -> anyhow::Result<ObjectStoreConfig> {
    let proof_store_dir = config.proof_store_dir.clone();
    let prover_path = get_link_to_prover(ecosystem_config);

    let proof_store_dir = prover_path.join(proof_store_dir).join("witness_inputs");

    let cmd = Cmd::new(cmd!(shell, "mkdir -p {proof_store_dir}"));
    cmd.run()?;

    let object_store_config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: config.proof_store_dir,
        },
        max_retries: PROVER_STORE_MAX_RETRIES,
        local_mirror_path: None,
    };

    Ok(object_store_config)
}
