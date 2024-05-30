use clap::Parser;
use common::{slugify, Prompt};
use config::{ChainConfig, DatabaseConfig, DatabasesConfig};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::defaults::{generate_db_names, DBNames, DATABASE_PROVER_URL, DATABASE_SERVER_URL};
use crate::messages::{
    msg_prover_db_name_prompt, msg_prover_db_url_prompt, msg_server_db_name_prompt,
    msg_server_db_url_prompt, MSG_GENESIS_USE_DEFAULT_HELP, MSG_PROVER_DB_NAME_HELP,
    MSG_PROVER_DB_URL_HELP, MSG_SERVER_DB_NAME_HELP, MSG_SERVER_DB_URL_HELP,
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct GenesisArgs {
    #[clap(long, help = MSG_SERVER_DB_URL_HELP)]
    pub server_db_url: Option<String>,
    #[clap(long, help = MSG_SERVER_DB_NAME_HELP)]
    pub server_db_name: Option<String>,
    #[clap(long, help = MSG_PROVER_DB_URL_HELP)]
    pub prover_db_url: Option<String>,
    #[clap(long, help = MSG_PROVER_DB_NAME_HELP)]
    pub prover_db_name: Option<String>,
    #[clap(long, short, help = MSG_GENESIS_USE_DEFAULT_HELP)]
    pub use_default: bool,
    #[clap(long, short, action)]
    pub dont_drop: bool,
}

impl GenesisArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> GenesisArgsFinal {
        let DBNames {
            server_name,
            prover_name,
        } = generate_db_names(config);
        let chain_name = config.name.clone();
        if self.use_default {
            GenesisArgsFinal {
                server_db_url: DATABASE_SERVER_URL.to_string(),
                server_db_name: server_name,
                prover_db_url: DATABASE_PROVER_URL.to_string(),
                prover_db_name: prover_name,
                dont_drop: self.dont_drop,
            }
        } else {
            let server_db_url = self.server_db_url.unwrap_or_else(|| {
                Prompt::new(&msg_server_db_url_prompt(&chain_name))
                    .default(DATABASE_SERVER_URL)
                    .ask()
            });
            let server_db_name = slugify(&self.server_db_name.unwrap_or_else(|| {
                Prompt::new(&msg_server_db_name_prompt(&chain_name))
                    .default(&server_name)
                    .ask()
            }));
            let prover_db_url = self.prover_db_url.unwrap_or_else(|| {
                Prompt::new(&msg_prover_db_url_prompt(&chain_name))
                    .default(DATABASE_PROVER_URL)
                    .ask()
            });
            let prover_db_name = slugify(&self.prover_db_name.unwrap_or_else(|| {
                Prompt::new(&msg_prover_db_name_prompt(&chain_name))
                    .default(&prover_name)
                    .ask()
            }));
            GenesisArgsFinal {
                server_db_url,
                server_db_name,
                prover_db_url,
                prover_db_name,
                dont_drop: self.dont_drop,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisArgsFinal {
    pub server_db_url: String,
    pub server_db_name: String,
    pub prover_db_url: String,
    pub prover_db_name: String,
    pub dont_drop: bool,
}

impl GenesisArgsFinal {
    pub fn databases_config(&self) -> anyhow::Result<DatabasesConfig> {
        let server_url = Url::parse(&self.server_db_url)?;
        let prover_url = Url::parse(&self.prover_db_url)?;

        Ok(DatabasesConfig {
            server: DatabaseConfig::new(server_url, self.server_db_name.clone()),
            prover: DatabaseConfig::new(prover_url, self.prover_db_name.clone()),
        })
    }
}
