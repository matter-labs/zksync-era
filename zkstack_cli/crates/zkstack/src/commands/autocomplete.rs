use std::{
    fs::File,
    io::{BufWriter, Write},
};

use anyhow::Context;
use clap::CommandFactory;
use clap_complete::{generate, Generator};
use zkstack_cli_common::logger;

use super::args::AutocompleteArgs;
use crate::{
    messages::{msg_generate_autocomplete_file, MSG_OUTRO_AUTOCOMPLETE_GENERATION},
    ZkStack,
};

pub fn run(args: AutocompleteArgs) -> anyhow::Result<()> {
    let filename = autocomplete_file_name(&args.generator);
    let path = args.out.join(filename);

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
    let mut cmd = ZkStack::command();
    let cmd_name = cmd.get_name().to_string();

    generate(gen, &mut cmd, cmd_name, buf);

    Ok(())
}

pub fn autocomplete_file_name(shell: &clap_complete::Shell) -> &'static str {
    match shell {
        clap_complete::Shell::Bash => "zkstack.sh",
        clap_complete::Shell::Fish => "zkstack.fish",
        clap_complete::Shell::Zsh => "_zkstack.zsh",
        _ => todo!(),
    }
}
