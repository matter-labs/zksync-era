use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use url::Url;
use zkstack_cli_common::{db::DatabaseConfig, Prompt};
use zkstack_cli_config::ChainConfig;

use crate::{
    defaults::{generate_db_names, DBNames, DATABASE_SERVER_URL},
    messages::{
        msg_server_db_name_prompt, msg_server_db_url_prompt, MSG_SERVER_COMMAND_HELP,
        MSG_SERVER_DB_NAME_HELP, MSG_SERVER_DB_URL_HELP, MSG_USE_DEFAULT_DATABASES_HELP,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct GenesisArgs {
    #[clap(long, help = MSG_SERVER_DB_URL_HELP)]
    pub server_db_url: Option<Url>,
    #[clap(long, help = MSG_SERVER_DB_NAME_HELP)]
    pub server_db_name: Option<String>,
    #[clap(long, help = "Database URL to use as template for creating the new database")]
    pub db_template: Option<Url>,
    #[clap(long, short, help = MSG_USE_DEFAULT_DATABASES_HELP)]
    pub dev: bool,
    #[clap(long, short, action)]
    pub dont_drop: bool,
    #[clap(long, help = MSG_SERVER_COMMAND_HELP)]
    pub server_command: Option<String>,
}

impl GenesisArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> GenesisArgsFinal {
        let DBNames { server_name, .. } = generate_db_names(config);
        let chain_name = config.name.clone();
        if self.dev {
            GenesisArgsFinal {
                server_db: DatabaseConfig::new(DATABASE_SERVER_URL.clone(), server_name),
                dont_drop: self.dont_drop,
                server_command: self.server_command,
                db_template: self.db_template,
            }
        } else {
            let server_db_url = self.server_db_url.unwrap_or_else(|| {
                Prompt::new(&msg_server_db_url_prompt(&chain_name))
                    .default(DATABASE_SERVER_URL.as_str())
                    .ask()
            });
            let server_db_name = slugify!(
                &self.server_db_name.unwrap_or_else(|| {
                    Prompt::new(&msg_server_db_name_prompt(&chain_name))
                        .default(&server_name)
                        .ask()
                }),
                separator = "_"
            );
            GenesisArgsFinal {
                server_db: DatabaseConfig::new(server_db_url, server_db_name),
                dont_drop: self.dont_drop,
                server_command: self.server_command,
                db_template: self.db_template,
            }
        }
    }

    pub async fn fill_values_with_secrets(
        mut self,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<GenesisArgsFinal> {
        let secrets = chain_config.get_secrets_config().await?;
        let server_url = secrets.core_database_url()?;

        let (server_db_url, server_db_name) = if let Some(db_full_url) = &server_url {
            let db_config =
                DatabaseConfig::from_url(db_full_url).context("Invalid server database URL")?;
            (Some(db_config.url), Some(db_config.name))
        } else {
            (None, None)
        };

        self.server_db_url = self.server_db_url.or(server_db_url);
        self.server_db_name = self.server_db_name.or(server_db_name);

        Ok(self.fill_values_with_prompt(chain_config))
    }

    pub fn reset_db_names(&mut self) {
        self.server_db_name = None;
        self.server_db_url = None;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisArgsFinal {
    pub server_command: Option<String>,
    pub server_db: DatabaseConfig,
    pub dont_drop: bool,
    pub db_template: Option<Url>,
}
