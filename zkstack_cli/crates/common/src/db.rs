use std::{collections::HashMap, path::PathBuf};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use sqlx::{
    migrate::{Migrate, MigrateError, Migrator},
    Connection, PgConnection,
};
use url::Url;
use xshell::Shell;

use crate::{config::global_config, logger};

/// Database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database URL.
    pub url: Url,
    /// Database name.
    pub name: String,
}

impl DatabaseConfig {
    /// Create a new `Db` instance.
    pub fn new(url: Url, name: String) -> Self {
        Self { url, name }
    }

    /// Create a new `Db` instance from a URL.
    pub fn from_url(url: &Url) -> anyhow::Result<Self> {
        let name = url
            .path_segments()
            .ok_or(anyhow!("Failed to parse database name from URL"))?
            .next_back()
            .ok_or(anyhow!("Failed to parse database name from URL"))?;
        let url_without_db_name = {
            let mut url = url.clone();
            url.set_path("");
            url
        };
        Ok(Self {
            url: url_without_db_name,
            name: name.to_string(),
        })
    }

    /// Get the full URL of the database.
    pub fn full_url(&self) -> Url {
        let mut url = self.url.clone();
        url.set_path(&self.name);
        url
    }
}

pub async fn init_db(db: &DatabaseConfig) -> anyhow::Result<()> {
    // Connect to the database.
    let mut connection = PgConnection::connect(db.url.as_str()).await?;

    let query = format!("CREATE DATABASE {}", db.name);
    // Create DB.
    sqlx::query(&query).execute(&mut connection).await?;

    Ok(())
}

pub async fn init_db_with_template(db: &DatabaseConfig, template_url: &Url) -> anyhow::Result<()> {
    // Connect to the database.
    let mut connection = PgConnection::connect(db.url.as_str()).await?;

    // Extract template database name from the template URL
    let template_db_name = template_url
        .path_segments()
        .ok_or(anyhow!("Failed to parse template database name from URL"))?
        .next_back()
        .ok_or(anyhow!("Failed to parse template database name from URL"))?;

    let query = format!("CREATE DATABASE \"{}\" WITH TEMPLATE \"{}\"", db.name, template_db_name);
    // Create DB with template.
    sqlx::query(&query).execute(&mut connection).await?;

    Ok(())
}

pub async fn drop_db_if_exists(db: &DatabaseConfig) -> anyhow::Result<()> {
    // Connect to the database.
    let mut connection = PgConnection::connect(db.url.as_str()).await?;

    let query = format!("DROP DATABASE IF EXISTS {}", db.name);
    // DROP DB.
    sqlx::query(&query).execute(&mut connection).await?;

    Ok(())
}

pub async fn migrate_db(
    shell: &Shell,
    migrations_folder: PathBuf,
    db_url: &Url,
) -> anyhow::Result<()> {
    // Most of this file is copy-pasted from SQLx CLI:
    // https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/src/migrate.rs
    // Warrants a refactoring if this tool makes it to production.

    if !shell.path_exists(&migrations_folder) {
        anyhow::bail!("Migrations folder {migrations_folder:?} doesn't exist");
    }
    let migrator = Migrator::new(migrations_folder).await?;

    let mut conn = PgConnection::connect(db_url.as_str()).await?;
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
                    logger::step(format!(
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

pub async fn wait_for_db(db_url: &Url, tries: u32) -> anyhow::Result<()> {
    for i in 0..tries {
        if PgConnection::connect(db_url.as_str()).await.is_ok() {
            return Ok(());
        }
        if i < tries - 1 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
    anyhow::bail!("Unable to connect to Postgres, connection cannot be established");
}
