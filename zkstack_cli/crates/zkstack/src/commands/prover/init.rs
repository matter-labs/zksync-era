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
    copy_configs, get_link_to_prover, EcosystemConfig, ObjectStoreConfig, ObjectStoreMode,
};

use super::{
    args::init::{ProofStorageConfig, ProofStorageFileBacked, ProverInitArgs},
    compressor_keys::{get_default_compressor_keys_path, run as compressor_keys},
    gcs::create_gcs_bucket,
    init_bellman_cuda::run as init_bellman_cuda,
    setup_keys,
};
use crate::{
    commands::prover::deploy_proving_network::deploy_proving_network,
    consts::{PROVER_MIGRATIONS, PROVER_STORE_MAX_RETRIES},
    messages::{
        MSG_CHAIN_NOT_FOUND_ERR, MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR,
        MSG_INITIALIZING_DATABASES_SPINNER, MSG_INITIALIZING_PROVER_DATABASE,
        MSG_PROVER_INITIALIZED,
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
        || chain_config.get_wallets_config().is_err()
        || chain_config.get_contracts_config().is_err()
    {
        copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;
    }

    let mut general_config = chain_config.get_general_config().await?.patched();

    let proof_object_store_config =
        get_object_store_config(shell, Some(args.proof_store))?.unwrap();

    if let Some(args) = args.deploy_proving_network {
        deploy_proving_network(shell, &ecosystem_config, &chain_config, args).await?;
    }

    if let Some(args) = args.bellman_cuda_config {
        init_bellman_cuda(shell, args).await?;
    }

    if let Some(args) = args.compressor_key_args {
        compressor_keys(shell, args).await?;
    }

    general_config.set_prover_object_store(&proof_object_store_config)?;
    general_config.save().await?;

    if let Some(args) = args.setup_keys {
        setup_keys::run(args, shell).await?;
    }

    if let Some(prover_db) = &args.database_config {
        let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);

        let mut secrets = chain_config.get_secrets_config().await?.patched();
        secrets.set_prover_database(&prover_db.database_config)?;
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
        }),
        Some(ProofStorageConfig::GCSCreateBucket(config)) => {
            Some(create_gcs_bucket(shell, config)?)
        }
        None => None,
    };

    Ok(object_store)
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
    };

    Ok(object_store_config)
}
