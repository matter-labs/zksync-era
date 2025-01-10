use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use strum::EnumIter;
use zkstack_cli_common::Prompt;

use crate::messages::MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT;

#[derive(Debug, Clone, Parser, Default)]
pub struct CompressorKeysArgs {
    #[clap(long)]
    pub plonk_path: Option<PathBuf>,
    #[clap(long)]
    pub fflonk_path: Option<PathBuf>,
    #[clap(long, default_value = "plonk")]
    pub compressor_type: CompressorType,
}

#[derive(Debug, Clone, ValueEnum, EnumIter, strum::Display, PartialEq, Eq, Default)]
pub enum CompressorType {
    Fflonk,
    #[default]
    Plonk,
    All,
}

impl CompressorKeysArgs {
    pub fn fill_values_with_prompt(
        self,
        default_plonk_path: &Path,
        default_fflonk_path: &Path,
    ) -> CompressorKeysArgs {
        let plonk_path = if self.compressor_type != CompressorType::Fflonk {
            Some(self.plonk_path.unwrap_or_else(|| {
                Prompt::new(MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT)
                    .default(default_plonk_path.to_str().expect("non-UTF8 PLONK path"))
                    .ask()
            }))
        } else {
            None
        };

        let fflonk_path = if self.compressor_type != CompressorType::Plonk {
            Some(self.fflonk_path.unwrap_or_else(|| {
                Prompt::new(MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT)
                    .default(default_fflonk_path.to_str().expect("non-UTF8 FFLONK path"))
                    .ask()
            }))
        } else {
            None
        };

        CompressorKeysArgs {
            plonk_path,
            fflonk_path,
            ..self
        }
    }
}
