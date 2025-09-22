use std::path::{Path, PathBuf};

use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger, spinner::Spinner};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};

use super::args::{init::InitContractVerifierArgs, releases::Version};
use crate::messages::{msg_binary_already_exists, msg_downloading_binary_spinner};

pub(crate) async fn run(shell: &Shell, args: InitContractVerifierArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt(shell)?;
    // Todo allow to provide link to code as an argument
    let ecosystem = ZkStackConfig::ecosystem(shell)?;
    let link_to_code = ecosystem.link_to_code();

    download_binaries(
        shell,
        args.zksolc_releases,
        get_zksolc_path,
        &link_to_code,
        "zksolc",
    )?;

    download_binaries(
        shell,
        args.zkvyper_releases,
        get_zkvyper_path,
        &link_to_code,
        "zkvyper",
    )?;

    download_binaries(
        shell,
        args.solc_releases,
        get_solc_path,
        &link_to_code,
        "solc",
    )?;

    download_binaries(
        shell,
        args.era_vm_solc_releases,
        get_era_vm_solc_path,
        &link_to_code,
        "solc",
    )?;

    download_binaries(
        shell,
        args.vyper_releases,
        get_vyper_path,
        &link_to_code,
        "vyper",
    )?;

    Ok(())
}

fn download_binaries(
    shell: &Shell,
    releases: Vec<Version>,
    get_path: fn(&Path, &str) -> PathBuf,
    link_to_code: &Path,
    name: &str,
) -> anyhow::Result<()> {
    for release in releases {
        download_binary(
            shell,
            &release.url,
            &get_path(link_to_code, &release.version),
            name,
            &release.version,
        )?;
    }
    Ok(())
}

fn download_binary(
    shell: &Shell,
    url: &str,
    path: &Path,
    name: &str,
    version: &str,
) -> anyhow::Result<()> {
    let binary_path = path.join(name);
    if shell.path_exists(binary_path.clone()) {
        logger::info(msg_binary_already_exists(name, version));
        return Ok(());
    }

    let spinner = Spinner::new(&msg_downloading_binary_spinner(name, version));
    Cmd::new(cmd!(shell, "mkdir -p {path}")).run()?;
    let response = reqwest::blocking::get(url)?.bytes()?;
    shell.write_file(binary_path.clone(), &response)?;
    Cmd::new(cmd!(shell, "chmod +x {binary_path}")).run()?;
    spinner.finish();

    Ok(())
}

fn get_zksolc_path(link_to_code: &Path, version: &str) -> PathBuf {
    link_to_code.join("etc/zksolc-bin/").join(version)
}

fn get_zkvyper_path(link_to_code: &Path, version: &str) -> PathBuf {
    link_to_code.join("etc/zkvyper-bin/").join(version)
}

fn get_vyper_path(link_to_code: &Path, version: &str) -> PathBuf {
    link_to_code
        .join("etc/vyper-bin/")
        .join(version.replace('v', ""))
}

fn get_solc_path(link_to_code: &Path, version: &str) -> PathBuf {
    link_to_code.join("etc/solc-bin/").join(version)
}

fn get_era_vm_solc_path(link_to_code: &Path, version: &str) -> PathBuf {
    link_to_code
        .join("etc/solc-bin/")
        .join(format!("zkVM-{version}"))
}
