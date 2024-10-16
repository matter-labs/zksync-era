use anyhow::Context;
use clap::Parser;
use common::{db::DatabaseConfig, Prompt};
use config::ChainConfig;
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use url::Url;

use crate::{
    defaults::{generate_db_names, DBNames, DATABASE_SERVER_URL},
    messages::{
        msg_server_db_name_prompt, msg_server_db_url_prompt, MSG_SERVER_DB_NAME_HELP,
        MSG_SERVER_DB_URL_HELP, MSG_USE_DEFAULT_DATABASES_HELP,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct GenesisArgs {
    #[clap(long, help = MSG_SERVER_DB_URL_HELP)]
    pub server_db_url: Option<Url>,
    #[clap(long, help = MSG_SERVER_DB_NAME_HELP)]
    pub server_db_name: Option<String>,
    #[clap(long, short, help = MSG_USE_DEFAULT_DATABASES_HELP)]
    pub use_default: bool,
    #[clap(long, short, action)]
    pub dont_drop: bool,
}

impl GenesisArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> GenesisArgsFinal {
        let DBNames { server_name, .. } = generate_db_names(config);
        let chain_name = config.name.clone();
        if self.use_default {
            GenesisArgsFinal {
                server_db: DatabaseConfig::new(DATABASE_SERVER_URL.clone(), server_name),
                dont_drop: self.dont_drop,
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
            }
        }
    }

    pub fn fill_values_with_secrets(
        mut self,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<GenesisArgsFinal> {
        let secrets = chain_config.get_secrets_config()?;
        let database = secrets
            .database
            .context("Database secrets must be present")?;

        let (server_db_url, server_db_name) = if let Some(db_full_url) = database.server_url {
            let db_config = DatabaseConfig::from_url(db_full_url.expose_url())
                .context("Invalid server database URL")?;
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
    pub server_db: DatabaseConfig,
    pub dont_drop: bool,
}
