use clap::Parser;
use common::{db::DatabaseConfig, Prompt};
use config::ChainConfig;
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use url::Url;

use crate::{
    defaults::{generate_external_node_db_name, DATABASE_SERVER_URL, LOCAL_RPC_URL},
    messages::{
        msg_external_node_db_name_prompt, msg_external_node_db_url_prompt, MSG_L1_RPC_URL_PROMPT,
        MSG_USE_DEFAULT_DATABASES_HELP,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct PrepareConfigArgs {
    #[clap(long)]
    pub db_url: Option<Url>,
    #[clap(long)]
    pub db_name: Option<String>,
    #[clap(long)]
    pub l1_rpc_url: Option<String>,
    #[clap(long, short, help = MSG_USE_DEFAULT_DATABASES_HELP)]
    pub use_default: bool,
}

impl PrepareConfigArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> PrepareConfigFinal {
        let db_name = generate_external_node_db_name(config);
        let chain_name = config.name.clone();
        if self.use_default {
            PrepareConfigFinal {
                db: DatabaseConfig::new(DATABASE_SERVER_URL.clone(), db_name),
                l1_rpc_url: LOCAL_RPC_URL.to_string(),
            }
        } else {
            let db_url = self.db_url.unwrap_or_else(|| {
                Prompt::new(&msg_external_node_db_url_prompt(&chain_name))
                    .default(DATABASE_SERVER_URL.as_str())
                    .ask()
            });
            let db_name = slugify!(
                &self.db_name.unwrap_or_else(|| {
                    Prompt::new(&msg_external_node_db_name_prompt(&chain_name))
                        .default(&db_name)
                        .ask()
                }),
                separator = "_"
            );
            let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
                Prompt::new(MSG_L1_RPC_URL_PROMPT)
                    .default(LOCAL_RPC_URL)
                    .ask()
            });

            PrepareConfigFinal {
                db: DatabaseConfig::new(db_url, db_name),
                l1_rpc_url,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareConfigFinal {
    pub db: DatabaseConfig,
    pub l1_rpc_url: String,
}
