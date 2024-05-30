use clap::Parser;
use common::{db::DatabaseConfig, slugify, Prompt};
use config::ChainConfig;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::defaults::{generate_db_names, DBNames, DATABASE_PROVER_URL, DATABASE_SERVER_URL};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct GenesisArgs {
    #[clap(long, help = "Server database url without database name")]
    pub server_db_url: Option<Url>,
    #[clap(long, help = "Server database name")]
    pub server_db_name: Option<String>,
    #[clap(long, help = "Prover database url without database name")]
    pub prover_db_url: Option<Url>,
    #[clap(long, help = "Prover database name")]
    pub prover_db_name: Option<String>,
    #[clap(long, short, help = "Use default database urls and names")]
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
                server_db: DatabaseConfig::new(DATABASE_SERVER_URL.clone(), server_name),
                prover_db: DatabaseConfig::new(DATABASE_PROVER_URL.clone(), prover_name),
                dont_drop: self.dont_drop,
            }
        } else {
            let server_db_url = self.server_db_url.unwrap_or_else(|| {
                Prompt::new(&format!(
                    "Please provide server database url for chain {chain_name}"
                ))
                .default(DATABASE_SERVER_URL.as_str())
                .ask()
            });
            let server_db_name = slugify(&self.server_db_name.unwrap_or_else(|| {
                Prompt::new(&format!(
                    "Please provide server database name for chain {chain_name}"
                ))
                .default(&server_name)
                .ask()
            }));
            let prover_db_url = self.prover_db_url.unwrap_or_else(|| {
                Prompt::new(&format!(
                    "Please provide prover database url for chain {chain_name}"
                ))
                .default(DATABASE_PROVER_URL.as_str())
                .ask()
            });
            let prover_db_name = slugify(&self.prover_db_name.unwrap_or_else(|| {
                Prompt::new(&format!(
                    "Please provide prover database name for chain {chain_name}"
                ))
                .default(&prover_name)
                .ask()
            }));
            GenesisArgsFinal {
                server_db: DatabaseConfig::new(server_db_url, server_db_name),
                prover_db: DatabaseConfig::new(prover_db_url, prover_db_name),
                dont_drop: self.dont_drop,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisArgsFinal {
    pub server_db: DatabaseConfig,
    pub prover_db: DatabaseConfig,
    pub dont_drop: bool,
}
