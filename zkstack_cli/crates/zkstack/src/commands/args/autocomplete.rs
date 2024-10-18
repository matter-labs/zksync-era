use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct AutocompleteArgs {
    /// The shell to generate the autocomplete script for
    #[arg(long = "generate", value_enum)]
    pub generator: clap_complete::Shell,
    /// The out directory to write the autocomplete script to
    #[arg(short, long, default_value = "./")]
    pub out: PathBuf,
}
