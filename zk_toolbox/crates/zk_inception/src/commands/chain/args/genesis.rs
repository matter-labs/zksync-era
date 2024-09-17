use clap::Parser;
use serde::{Deserialize, Serialize};
use config::ChainConfig;
use crate::{
    commands::chain::args::database::{DatabaseArgs, DatabaseArgsFinal},
    messages::{MSG_USE_DEFAULT_DATABASES_HELP},
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct GenesisArgs {
    #[clap(flatten)]
    #[serde(flatten)]
    pub database_args: DatabaseArgs,
    #[clap(long, short, help = MSG_USE_DEFAULT_DATABASES_HELP)]
    pub use_default: bool,
}

impl GenesisArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> GenesisArgsFinal {
        GenesisArgsFinal {
            database_args: self.database_args.genesis_fill_values_with_prompt(config, self.use_default),
            use_default: self.use_default,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisArgsFinal {
    pub database_args: DatabaseArgsFinal,
    pub use_default: bool,
}
