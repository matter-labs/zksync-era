use std::path::PathBuf;

use anyhow::Context;
use common::{
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    logger,
    spinner::Spinner,
};
use config::{traits::SaveConfigWithBasePath, GeneralProverConfig};
use xshell::Shell;
use zksync_basic_types::url::SensitiveUrl;

use crate::{
    commands::prover::args::setup_database::{SetupDatabaseArgs, SetupDatabaseArgsFinal},
    consts::PROVER_MIGRATIONS,
    messages::{
        MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR, MSG_INITIALIZING_DATABASES_SPINNER,
        MSG_SELECTED_CONFIG,
    },
};

pub async fn run(shell: &Shell, args: SetupDatabaseArgs) -> anyhow::Result<()> {
    let args = args.get_database_config_with_prompt();
    setup_database(shell, args).await?;
    Ok(())
}

pub async fn setup_database(shell: &Shell, args: SetupDatabaseArgsFinal) -> anyhow::Result<()> {
    let general_prover_config = GeneralProverConfig::from_file(shell)?;
    let mut secrets = general_prover_config.load_secrets_config()?;

    let database = secrets
        .database
        .as_mut()
        .context("Databases must be presented")?;
    database.prover_url = Some(SensitiveUrl::from(args.database_config.full_url()));

    secrets.save_with_base_path(shell, &general_prover_config.config)?;

    logger::note(
        MSG_SELECTED_CONFIG,
        logger::object_to_string(serde_json::json!({
            "prover_db_config": args.database_config,
        })),
    );

    let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    initialize_prover_db(
        shell,
        &args.database_config,
        general_prover_config.link_to_code.clone(),
        args.dont_drop,
    )
    .await?;
    spinner.finish();
    Ok(())
}

pub async fn initialize_prover_db(
    shell: &Shell,
    prover_db_config: &DatabaseConfig,
    link_to_code: PathBuf,
    dont_drop: bool,
) -> anyhow::Result<()> {
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
