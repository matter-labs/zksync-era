use crate::commands::prover::args::run::{
    ProverComponent, WitnessGeneratorArgs, WitnessVectorGeneratorArgs,
};
use crate::messages::{MSG_SETUP_KEYS_DOWNLOAD_HELP, MSG_SETUP_KEYS_REGION_PROMPT};
use clap::{Parser, ValueEnum};
use common::PromptSelect;
use strum::EnumIter;

#[derive(Debug, Clone, Parser, Default)]
pub struct SetupKeysArgs {
    #[clap(long)]
    pub region: Option<Region>,
    #[clap(long)]
    pub approach: Option<SetupKeysApproach>,
}

#[derive(Debug, Clone)]
pub struct SetupKeysArgsFinal {
    pub region: Option<Region>,
    pub approach: SetupKeysApproach,
}

#[derive(Debug, Clone, ValueEnum, strum::EnumString, EnumIter, PartialEq, Eq, strum::Display)]
pub(crate) enum SetupKeysApproach {
    Download,
    Generate,
}

#[derive(Debug, Clone, ValueEnum, strum::EnumString, EnumIter, PartialEq, Eq, strum::Display)]
pub(crate) enum Region {
    US,
    EUROPE,
    ASIA,
}

impl SetupKeysArgs {
    pub fn fill_values_with_prompt(self) -> SetupKeysArgsFinal {
        let approach = self.approach.unwrap_or_else(|| {
            PromptSelect::new(MSG_SETUP_KEYS_DOWNLOAD_HELP, SetupKeysApproach::iter()).ask()
        });

        if approach == SetupKeysApproach::Download {
            let region = self.region.unwrap_or_else(|| {
                PromptSelect::new(MSG_SETUP_KEYS_REGION_PROMPT, Region::iter()).ask()
            });

            SetupKeysArgsFinal {
                region: Some(region),
                approach,
            }
        } else {
            SetupKeysArgsFinal {
                region: None,
                approach,
            }
        }
    }
}
