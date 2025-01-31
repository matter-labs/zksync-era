use clap::{Parser, ValueEnum};
use strum::{EnumIter, IntoEnumIterator};
use zkstack_cli_common::PromptSelect;

use crate::messages::{MSG_SETUP_KEYS_DOWNLOAD_SELECTION_PROMPT, MSG_SETUP_KEYS_REGION_PROMPT};

#[derive(Debug, Clone, Parser, Default)]
pub struct SetupKeysArgs {
    #[clap(long)]
    pub region: Option<Region>,
    #[clap(long)]
    pub mode: Option<Mode>,
}

#[derive(Debug, Clone)]
pub struct SetupKeysArgsFinal {
    pub region: Option<Region>,
    pub mode: Mode,
}

#[derive(Debug, Clone, ValueEnum, strum::EnumString, EnumIter, PartialEq, Eq, strum::Display)]
pub enum Mode {
    Download,
    Generate,
}

#[derive(Debug, Clone, ValueEnum, strum::EnumString, EnumIter, PartialEq, Eq, strum::Display)]
pub enum Region {
    Us,
    Europe,
    Asia,
}

impl SetupKeysArgs {
    pub fn fill_values_with_prompt(self) -> SetupKeysArgsFinal {
        let mode = self.mode.unwrap_or_else(|| {
            PromptSelect::new(MSG_SETUP_KEYS_DOWNLOAD_SELECTION_PROMPT, Mode::iter()).ask()
        });

        if mode == Mode::Download {
            let region = self.region.unwrap_or_else(|| {
                PromptSelect::new(MSG_SETUP_KEYS_REGION_PROMPT, Region::iter()).ask()
            });

            SetupKeysArgsFinal {
                region: Some(region),
                mode,
            }
        } else {
            SetupKeysArgsFinal { region: None, mode }
        }
    }
}
