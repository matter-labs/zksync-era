use std::{fs::File, io::BufWriter, path::PathBuf};

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
    let mut cmd = Inception::command();
    let filename = format!(
        "_{}_{}",
        cmd.get_name(),
        args.generator.to_string().to_ascii_lowercase()
    );
    let path = args.out.join(&filename);

    logger::info(msg_generate_autocomplete_file(
        path.to_str()
            .context("the output file path is an invalid UTF8 string")?,
    ));

    export_completions(args.generator, &mut cmd, &path)?;

    logger::outro(MSG_OUTRO_AUTOCOMPLETE_GENERATION);

    Ok(())
}

fn export_completions<G: Generator>(
    gen: G,
    cmd: &mut clap::Command,
    path: &PathBuf,
) -> anyhow::Result<()> {
    let file = File::create(path).context("Failed to create file")?;
    let mut writer = BufWriter::new(file);

    generate(gen, cmd, cmd.get_name().to_string(), &mut writer);

    Ok(())
}
