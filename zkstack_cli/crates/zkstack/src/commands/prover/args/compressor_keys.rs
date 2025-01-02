use clap::{Parser, ValueEnum};
use common::Prompt;
use strum::EnumIter;

use crate::messages::MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT;

#[derive(Debug, Clone, Parser, Default)]
pub struct CompressorKeysArgs {
    #[clap(long)]
    pub plonk_path: Option<String>,
    #[clap(long)]
    pub fflonk_path: Option<String>,
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
        default_plonk_path: &str,
        default_fflonk_path: &str,
    ) -> CompressorKeysArgs {
        let plonk_path = if self.compressor_type != CompressorType::Fflonk {
            Some(self.plonk_path.unwrap_or_else(|| {
                Prompt::new(MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT)
                    .default(default_plonk_path)
                    .ask()
            }))
        } else {
            None
        };

        let fflonk_path = if self.compressor_type != CompressorType::Plonk {
            Some(self.fflonk_path.unwrap_or_else(|| {
                Prompt::new(MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT)
                    .default(default_fflonk_path)
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
