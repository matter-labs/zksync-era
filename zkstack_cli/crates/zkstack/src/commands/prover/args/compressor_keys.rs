use clap::Parser;
use zkstack_cli_common::Prompt;

use crate::messages::MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT;

#[derive(Debug, Clone, Parser, Default)]
pub struct CompressorKeysArgs {
    #[clap(long)]
    pub path: Option<String>,
}

impl CompressorKeysArgs {
    pub fn fill_values_with_prompt(self, default_path: &str) -> CompressorKeysArgs {
        let path = self.path.unwrap_or_else(|| {
            Prompt::new(MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT)
                .default(default_path)
                .ask()
        });

        CompressorKeysArgs { path: Some(path) }
    }
}
