use anyhow::Context;
use clap::Parser;
use common::{
    spinner::{self, Spinner},
    PromptSelect,
};
use xshell::Shell;

use super::releases::{get_releases_with_arch, Arch, Version};
use crate::messages::{
    MSG_ARCH_NOT_SUPPORTED_ERR, MSG_FETCHING_VYPER_RELEASES_SPINNER,
    MSG_FETCHING_ZKSOLC_RELEASES_SPINNER, MSG_GET_VYPER_RELEASES_ERR, MSG_GET_ZKSOLC_RELEASES_ERR,
    MSG_NO_VERSION_FOUND_ERR, MSG_OS_NOT_SUPPORTED_ERR, MSG_VYPER_VERSION_PROMPT,
    MSG_ZKSOLC_VERSION_PROMPT,
};

#[derive(Debug, Clone, Parser, Default)]
pub struct InitContractVerifierArgs {
    /// Version of zksolc to install
    #[clap(long)]
    pub zksolc_version: Option<String>,
    /// Version of vyper to install
    #[clap(long)]
    pub vyper_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InitContractVerifierArgsFinal {
    pub zksolc_version: Version,
    pub vyper_version: Version,
}

impl InitContractVerifierArgs {
    pub fn fill_values_with_prompt(
        self,
        shell: &Shell,
    ) -> anyhow::Result<InitContractVerifierArgsFinal> {
        let arch = get_arch()?;

        let spinner = Spinner::new(MSG_FETCHING_ZKSOLC_RELEASES_SPINNER);
        let zksolc_releases = get_releases_with_arch(shell, "matter-labs/zksolc-bin", arch)
            .context(MSG_GET_ZKSOLC_RELEASES_ERR)?;
        spinner.finish();

        let spinner = spinner::Spinner::new(MSG_FETCHING_VYPER_RELEASES_SPINNER);
        let vyper_releases = get_releases_with_arch(shell, "vyperlang/vyper", arch)
            .context(MSG_GET_VYPER_RELEASES_ERR)?;
        spinner.finish();

        let zksolc_version = select_version(
            self.zksolc_version,
            zksolc_releases,
            MSG_ZKSOLC_VERSION_PROMPT,
        )?;

        let vyper_version =
            select_version(self.vyper_version, vyper_releases, MSG_VYPER_VERSION_PROMPT)?;

        Ok(InitContractVerifierArgsFinal {
            zksolc_version,
            vyper_version,
        })
    }
}

fn get_arch() -> anyhow::Result<Arch> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let arch = match os {
        "linux" => match arch {
            "x86_64" => Arch::LinuxAmd,
            "aarch64" => Arch::LinuxArm,
            "arm" => Arch::LinuxArm,
            _ => anyhow::bail!(MSG_ARCH_NOT_SUPPORTED_ERR),
        },
        "macos" => match arch {
            "x86_64" => Arch::MacosAmd,
            "aarch64" => Arch::MacosArm,
            "arm" => Arch::MacosArm,
            _ => anyhow::bail!(MSG_ARCH_NOT_SUPPORTED_ERR),
        },
        _ => anyhow::bail!(MSG_OS_NOT_SUPPORTED_ERR),
    };

    Ok(arch)
}

fn select_version(
    selected: Option<String>,
    versions: Vec<Version>,
    prompt_msg: &str,
) -> anyhow::Result<Version> {
    let selected = selected.unwrap_or_else(|| {
        PromptSelect::new(prompt_msg, versions.iter().map(|r| &r.version))
            .ask()
            .into()
    });

    let selected = versions
        .iter()
        .find(|r| r.version == selected)
        .context(MSG_NO_VERSION_FOUND_ERR)?
        .to_owned();

    Ok(selected)
}
