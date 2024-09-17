use clap::Parser;
use common::{db::DatabaseConfig, Prompt};
use config::ChainConfig;
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use url::Url;
use std::path::PathBuf;

use crate::{
    defaults::{generate_db_names, DBNames, DATABASE_PROVER_URL, DATABASE_SERVER_URL},
    messages::{
        msg_prover_db_name_prompt, msg_prover_db_url_prompt, msg_server_db_name_prompt,
        msg_server_db_url_prompt, MSG_PROVER_DB_NAME_HELP, MSG_PROVER_DB_URL_HELP,
        MSG_SERVER_DB_NAME_HELP, MSG_SERVER_DB_URL_HELP, MSG_DONT_DROP_HELP,
        MSG_LINK_TO_CODE_HELP,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct DatabaseArgs {
    #[clap(long, help = MSG_SERVER_DB_URL_HELP)]
    pub server_db_url: Option<Url>,
    #[clap(long, help = MSG_SERVER_DB_NAME_HELP)]
    pub server_db_name: Option<String>,
    #[clap(long, help = MSG_PROVER_DB_URL_HELP)]
    pub prover_db_url: Option<Url>,
    #[clap(long, help = MSG_PROVER_DB_NAME_HELP)]
    pub prover_db_name: Option<String>,
    #[clap(long, help = MSG_LINK_TO_CODE_HELP)]
    pub link_to_code: Option<String>,
    #[clap(long, short, action, help = MSG_DONT_DROP_HELP)]
    pub dont_drop: bool,
}

struct DBConfigs {
    server_db: DatabaseConfig,
    prover_db: DatabaseConfig,
}

impl DatabaseArgs {
    fn prompt_db_configs(self, chain_name: &str, server_name: &str, prover_name: &str) -> DBConfigs {
        let server_db_url = self.server_db_url.unwrap_or_else(|| {
            Prompt::new(&msg_server_db_url_prompt(chain_name))
                .default(DATABASE_SERVER_URL.as_str())
                .ask()
        });
        let server_db_name = slugify!(
            &self.server_db_name.unwrap_or_else(|| {
                Prompt::new(&msg_server_db_name_prompt(chain_name))
                    .default(server_name)
                    .ask()
            }),
            separator = "_"
        );
        let prover_db_url = self.prover_db_url.unwrap_or_else(|| {
            Prompt::new(&msg_prover_db_url_prompt(chain_name))
                .default(DATABASE_PROVER_URL.as_str())
                .ask()
        });
        let prover_db_name = slugify!(
            &self.prover_db_name.unwrap_or_else(|| {
                Prompt::new(&msg_prover_db_name_prompt(chain_name))
                    .default(prover_name)
                    .ask()
            }),
            separator = "_"
        );
        DBConfigs {
            server_db: DatabaseConfig::new(server_db_url, server_db_name),
            prover_db: DatabaseConfig::new(prover_db_url, prover_db_name),
        }
    }
    pub fn standalone_fill_values_with_prompt(self) -> DatabaseArgsFinal {
        let link_to_code = match self.link_to_code.clone() {
            Some(x) => PathBuf::from(x),
            None => panic!("--link-to-code is required when running the standalone database initializer. Dynamic checkout is not supported."),
        };
        let chain_name = "mychain";
        let server_name = "mychain_server";
        let prover_name = "mychain_prover";
        let dont_drop = self.dont_drop.clone();
        let DBConfigs {
            server_db,
            prover_db,
        } = self.prompt_db_configs(&chain_name, &server_name, &prover_name);
        
        DatabaseArgsFinal {
            server_db,
            prover_db,
            link_to_code,
            dont_drop,
        }
    }
    pub fn genesis_fill_values_with_prompt(self, config: &ChainConfig, use_default: bool) -> DatabaseArgsFinal {
        let DBNames {
            server_name,
            prover_name,
        } = generate_db_names(config);
        
        let link_to_code = match self.link_to_code.clone() {
            Some(x) => PathBuf::from(x),
            None => config.link_to_code.clone(),
        };
        let dont_drop = self.dont_drop.clone();
        
        if use_default {
            DatabaseArgsFinal {
                server_db: DatabaseConfig::new(DATABASE_SERVER_URL.clone(), server_name),
                prover_db: DatabaseConfig::new(DATABASE_PROVER_URL.clone(), prover_name),
                link_to_code,
                dont_drop,
            }
        } else {
            let DBConfigs {
                server_db,
                prover_db,
            } = self.prompt_db_configs(&config.name, &server_name, &prover_name);
            
            DatabaseArgsFinal {
                server_db,
                prover_db,
                link_to_code,
                dont_drop,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseArgsFinal {
    pub server_db: DatabaseConfig,
    pub prover_db: DatabaseConfig,
    pub link_to_code: PathBuf,
    pub dont_drop: bool,
}
