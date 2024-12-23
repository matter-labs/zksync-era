use clap::{Parser, ValueEnum};
use common::server::ExecutionMode;
use serde::{Deserialize, Serialize};

use crate::{
    messages::{
        MSG_DOCKER_IMAGE_TAG_OPTION, MSG_SERVER_ADDITIONAL_ARGS_HELP, MSG_SERVER_COMPONENTS_HELP,
        MSG_SERVER_GENESIS_HELP, MSG_SERVER_URING_HELP,
    },
    utils::docker::select_tag,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, ValueEnum)]
pub enum Mode {
    #[default]
    Release,
    Debug,
    Docker,
}

impl Mode {
    pub fn as_execution_mode(&self, tag: Option<String>) -> ExecutionMode {
        match self {
            Mode::Debug => ExecutionMode::Debug,
            Mode::Release => ExecutionMode::Release,
            Mode::Docker => ExecutionMode::Docker {
                tag: tag.unwrap_or("latest".to_string()),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct RunServerArgs {
    #[arg(long, default_value = "release")]
    pub mode: Mode,
    #[arg(long, help = MSG_DOCKER_IMAGE_TAG_OPTION)]
    pub tag: Option<String>,
    #[arg(long, help = MSG_SERVER_COMPONENTS_HELP)]
    pub components: Option<Vec<String>>,
    #[arg(long, help = MSG_SERVER_GENESIS_HELP)]
    pub genesis: bool,
    #[arg(
        long, short,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        hide = false,
        help = MSG_SERVER_ADDITIONAL_ARGS_HELP
    )]
    additional_args: Vec<String>,
    #[clap(help = MSG_SERVER_URING_HELP, long, default_missing_value = "true")]
    pub uring: bool,
}

impl RunServerArgs {
    pub async fn fill_values_with_prompt(self) -> RunServerArgsFinal {
        let tag = if let Mode::Docker = self.mode {
            self.tag
                .or(select_tag().await.ok().or(Some("latest".to_string())))
        } else {
            None
        };

        RunServerArgsFinal {
            mode: self.mode.as_execution_mode(tag),
            components: self.components,
            genesis: self.genesis,
            uring: self.uring,
        }
    }
}

#[derive(Debug)]
pub struct RunServerArgsFinal {
    pub mode: ExecutionMode,
    pub components: Option<Vec<String>>,
    pub genesis: bool,
    pub uring: bool,
}
