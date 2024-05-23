use std::{collections::HashMap, path::PathBuf};

use crate::{config::global_config, logger};
use sqlx::{
    migrate::{Migrate, MigrateError, Migrator},
    Connection, PgConnection,
};
use url::Url;
use xshell::Shell;

pub async fn init_db(db_url: &Url, name: &str) -> anyhow::Result<()> {
    // Connect to the database.
    let mut connection = PgConnection::connect(db_url.as_ref()).await?;

    let query = format!("CREATE DATABASE {}", name);
    // Create DB.
    sqlx::query(&query).execute(&mut connection).await?;

    Ok(())
}

pub async fn drop_db_if_exists(db_url: &Url, name: &str) -> anyhow::Result<()> {
    // Connect to the database.
    let mut connection = PgConnection::connect(db_url.as_ref()).await?;

    let query = format!("DROP DATABASE IF EXISTS {}", name);
    // DROP DB.
    sqlx::query(&query).execute(&mut connection).await?;

    Ok(())
}

pub async fn migrate_db(
    shell: &Shell,
    migrations_folder: PathBuf,
    db_url: &str,
) -> anyhow::Result<()> {
    // Most of this file is copy-pasted from SQLx CLI:
    // https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/src/migrate.rs
    // Warrants a refactoring if this tool makes it to production.

    if !shell.path_exists(&migrations_folder) {
        anyhow::bail!("Migrations folder {migrations_folder:?} doesn't exist");
    }
    let migrator = Migrator::new(migrations_folder).await?;

    let mut conn = PgConnection::connect(db_url).await?;
    conn.ensure_migrations_table().await?;

    let version = conn.dirty_version().await?;
    if let Some(version) = version {
        anyhow::bail!(MigrateError::Dirty(version));
    }

    let applied_migrations: HashMap<_, _> = conn
        .list_applied_migrations()
        .await?
        .into_iter()
        .map(|m| (m.version, m))
        .collect();

    if global_config().verbose {
        logger::debug("Migrations result:")
    }

    for migration in migrator.iter() {
        if migration.migration_type.is_down_migration() {
            // Skipping down migrations
            continue;
        }

        match applied_migrations.get(&migration.version) {
            Some(applied_migration) => {
                if migration.checksum != applied_migration.checksum {
                    anyhow::bail!(MigrateError::VersionMismatch(migration.version));
                }
            }
            None => {
                let skip = false;

                let elapsed = conn.apply(migration).await?;
                let text = if skip { "Skipped" } else { "Applied" };

                if global_config().verbose {
                    logger::raw(&format!(
                        "    {} {}/{} {} ({elapsed:?})",
                        text,
                        migration.version,
                        migration.migration_type.label(),
                        migration.description,
                    ));
                }
            }
        }
    }

    // Close the connection before exiting:
    // * For MySQL and Postgres this should ensure timely cleanup on the server side,
    //   including decrementing the open connection count.
    // * For SQLite this should checkpoint and delete the WAL file to ensure the migrations
    //   were actually applied to the database file and aren't just sitting in the WAL file.
    let _ = conn.close().await;

    Ok(())
}
