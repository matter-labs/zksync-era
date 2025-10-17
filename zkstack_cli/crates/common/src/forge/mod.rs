mod runner;
mod script;
use std::path::{Path, PathBuf};

use clap::Parser;
pub use runner::{ForgeRunner, ForgeRunnerArgs, ForgeScriptRun};
pub use script::{ForgeScript, ForgeScriptArg, ForgeScriptArgs, ForgeVerifier};
use serde::{Deserialize, Serialize};

/// Forge is a wrapper around the forge binary.
pub struct Forge {
    path: PathBuf,
}

impl Forge {
    /// Create a new Forge instance.
    pub fn new(path: &Path) -> Self {
        Forge {
            path: path.to_path_buf(),
        }
    }

    /// Create a new ForgeScript instance.
    ///
    /// The script path can be passed as a relative path to the base path
    /// or as an absolute path.
    pub fn script(&self, path: &Path, args: ForgeScriptArgs) -> ForgeScript {
        ForgeScript {
            base_path: self.path.clone(),
            script_path: path.to_path_buf(),
            args,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Parser, Clone)]
pub struct ForgeArgs {
    #[clap(flatten)]
    #[serde(flatten)]
    pub script: ForgeScriptArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub runner: ForgeRunnerArgs,
}

impl From<ForgeScriptArgs> for ForgeArgs {
    fn from(script: ForgeScriptArgs) -> Self {
        Self {
            script,
            runner: ForgeRunnerArgs::default(),
        }
    }
}

impl From<(ForgeScriptArgs, ForgeRunnerArgs)> for ForgeArgs {
    fn from((script, runner): (ForgeScriptArgs, ForgeRunnerArgs)) -> Self {
        Self { script, runner }
    }
}
