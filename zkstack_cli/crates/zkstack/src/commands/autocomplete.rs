use std::{fs::File, io::BufWriter};

use anyhow::Context;
use clap::CommandFactory;
use clap_complete::{generate, Generator};
use common::logger;

use crate::{
    messages::{msg_generate_autocomplete_file_for, MSG_OUTRO_AUTOCOMPLETE_GENERATION},
    Inception,
};

use super::args::AutocompleteArgs;

pub fn run(args: AutocompleteArgs) -> anyhow::Result<()> {
    let mut cmd = Inception::command();

    logger::info(msg_generate_autocomplete_file_for(cmd.get_name()));

    export_completions(args.generator, &mut cmd)?;

    logger::outro(MSG_OUTRO_AUTOCOMPLETE_GENERATION);

    Ok(())
}

fn export_completions<G: Generator>(gen: G, cmd: &mut clap::Command) -> anyhow::Result<()> {
    let file_name = format!("_{}", cmd.get_name());
    let file = File::create(file_name).context("Failed to create file")?;
    let mut writer = BufWriter::new(file);

    generate(gen, cmd, cmd.get_name().to_string(), &mut writer);

    Ok(())
}
