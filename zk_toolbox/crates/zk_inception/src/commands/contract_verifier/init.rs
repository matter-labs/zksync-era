use std::path::PathBuf;

use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{msg_binary_already_exists, msg_downloading_binary_spinner};

use super::args::init::InitContractVerifierArgs;

pub(crate) async fn run(shell: &Shell, args: InitContractVerifierArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt(shell)?;
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let link_to_code = ecosystem.link_to_code;
    let zksolc_path = link_to_code
        .join("etc/zksolc-bin/")
        .join(args.zksolc_version.version);
    let zkvyper_path = link_to_code
        .join("etc/zkvyper-bin/")
        .join(args.zkvyper_version.version);
    let vyper_path = link_to_code
        .join("etc/vyper-bin/")
        .join(args.vyper_version.version.replace("v", ""));

    download_binary(shell, &args.zksolc_version.url, &zksolc_path, "zksolc")?;
    download_binary(shell, &args.zkvyper_version.url, &zkvyper_path, "zkvyper")?;
    download_binary(shell, &args.vyper_version.url, &vyper_path, "vyper")?;

    Ok(())
}

fn download_binary(shell: &Shell, url: &str, path: &PathBuf, name: &str) -> anyhow::Result<()> {
    let binary_path = path.join(name);
    if shell.path_exists(binary_path.clone()) {
        logger::info(&msg_binary_already_exists(name));
        return Ok(());
    }

    let spinner = Spinner::new(&msg_downloading_binary_spinner(name));
    Cmd::new(cmd!(shell, "mkdir -p {path}")).run()?;
    Cmd::new(cmd!(shell, "wget {url} -O {binary_path}")).run()?;
    Cmd::new(cmd!(shell, "chmod +x {binary_path}")).run()?;
    spinner.finish();

    Ok(())
}
