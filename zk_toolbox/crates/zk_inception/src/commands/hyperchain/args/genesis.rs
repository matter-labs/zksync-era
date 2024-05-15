use clap::Parser;
use common::Prompt;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    configs::{DatabaseConfig, DatabasesConfig, HyperchainConfig},
    defaults::{generate_db_names, DBNames, DATABASE_PROVER_URL, DATABASE_SERVER_URL},
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct GenesisArgs {
    #[clap(long, help = "Server database url without database name")]
    pub server_db_url: Option<String>,
    #[clap(long, help = "Server database name")]
    pub server_db_name: Option<String>,
    #[clap(long, help = "Prover database url without database name")]
    pub prover_db_url: Option<String>,
    #[clap(long, help = "Prover database name")]
    pub prover_db_name: Option<String>,
    #[clap(long, short, help = "Use default database urls and names")]
    pub use_default: bool,
    #[clap(long, short, action)]
    pub dont_drop: bool,
}

impl GenesisArgs {
    pub fn fill_values_with_prompt(self, config: &HyperchainConfig) -> GenesisArgsFinal {
        let DBNames {
            server_name,
            prover_name,
        } = generate_db_names(config);
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
                Prompt::new("Please provide server database url")
                    .default(DATABASE_SERVER_URL)
                    .ask()
            });
            let server_db_name = self.server_db_name.unwrap_or_else(|| {
                Prompt::new("Please provide server database name")
                    .default(&server_name)
                    .ask()
            });
            let prover_db_url = self.prover_db_url.unwrap_or_else(|| {
                Prompt::new("Please provide prover database url")
                    .default(DATABASE_PROVER_URL)
                    .ask()
            });
            let prover_db_name = self.prover_db_name.unwrap_or_else(|| {
                Prompt::new("Please provide prover database name")
                    .default(&prover_name)
                    .ask()
            });
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
