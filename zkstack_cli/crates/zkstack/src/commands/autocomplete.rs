use std::{
    fs::File,
    io::{BufWriter, Write},
};

use anyhow::Context;
use clap::CommandFactory;
use clap_complete::{generate, Generator};
use common::logger;

use super::args::AutocompleteArgs;
use crate::{
    messages::{msg_generate_autocomplete_file, MSG_OUTRO_AUTOCOMPLETE_GENERATION},
    Inception,
};

pub fn run(args: AutocompleteArgs) -> anyhow::Result<()> {
    let path = args.out.join(args.generator.autocomplete_file_name());

    logger::info(msg_generate_autocomplete_file(
        path.to_str()
            .context("the output file path is an invalid UTF8 string")?,
    ));

    let file = File::create(path).context("Failed to create file")?;
    let mut writer = BufWriter::new(file);

    generate_completions(args.generator, &mut writer)?;

    logger::outro(MSG_OUTRO_AUTOCOMPLETE_GENERATION);

    Ok(())
}

pub fn generate_completions<G: Generator>(gen: G, buf: &mut dyn Write) -> anyhow::Result<()> {
    let mut cmd = Inception::command();
    let cmd_name = cmd.get_name().to_string();

    generate(gen, &mut cmd, cmd_name, buf);

    Ok(())
}

pub trait ShellAutocomplete {
    fn autocomplete_file_name(&self) -> String;
}

impl ShellAutocomplete for clap_complete::Shell {
    fn autocomplete_file_name(&self) -> String {
        match self {
            clap_complete::Shell::Bash => format!("zkstack.sh"),
            clap_complete::Shell::Fish => format!("zkstack.fish"),
            clap_complete::Shell::Zsh => format!("_zkstack.zsh"),
            _ => todo!(),
        }
    }
}
