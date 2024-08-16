use clap::Parser;
use common::{db::DatabaseConfig, Prompt};
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use url::Url;

use crate::{
    defaults::DATABASE_PROVER_URL,
    messages::{
        msg_prover_db_name_prover_only_prompt, msg_prover_db_url_prover_only_prompt,
        MSG_PROVER_DB_NAME_HELP, MSG_PROVER_DB_URL_HELP, MSG_USE_DEFAULT_DATABASES_HELP,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct SetupDatabaseArgs {
    #[clap(long, help = MSG_PROVER_DB_URL_HELP)]
    pub prover_db_url: Option<Url>,
    #[clap(long, help = MSG_PROVER_DB_NAME_HELP)]
    pub prover_db_name: Option<String>,
    #[clap(long, short, help = MSG_USE_DEFAULT_DATABASES_HELP)]
    pub use_default_db: bool,
    #[clap(long, short, action)]
    pub dont_drop_db: bool,
}

#[derive(Debug, Clone)]
pub struct SetupDatabaseArgsFinal {
    pub database_config: DatabaseConfig,
    pub dont_drop: bool,
}

impl SetupDatabaseArgs {
    pub fn get_database_config_with_prompt(&self) -> SetupDatabaseArgsFinal {
        let prover_name = "zksync_prover_local".to_string();
        if self.use_default_db {
            SetupDatabaseArgsFinal {
                database_config: DatabaseConfig::new(DATABASE_PROVER_URL.clone(), prover_name),
                dont_drop: self.dont_drop_db,
            }
        } else {
            let prover_db_url = self.prover_db_url.clone().unwrap_or_else(|| {
                Prompt::new(&msg_prover_db_url_prover_only_prompt())
                    .default(DATABASE_PROVER_URL.as_str())
                    .ask()
            });
            let prover_db_name = slugify!(
                &self.prover_db_name.clone().unwrap_or_else(|| {
                    Prompt::new(&msg_prover_db_name_prover_only_prompt())
                        .default(&prover_name)
                        .ask()
                }),
                separator = "_"
            );
            SetupDatabaseArgsFinal {
                database_config: DatabaseConfig::new(prover_db_url, prover_db_name),
                dont_drop: self.dont_drop_db,
            }
        }
    }
}
