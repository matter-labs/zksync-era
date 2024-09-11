use anyhow::Context;
use clap::Parser;
use common::PromptSelect;
use xshell::Shell;

use super::releases::{get_releases_with_arch, Arch, Version};
use crate::messages::{
    MSG_ARCH_NOT_SUPPORTED_ERR, MSG_FETCHING_VYPER_RELEASES_SPINNER,
    MSG_FETCHING_ZKSOLC_RELEASES_SPINNER, MSG_FETCHING_ZKVYPER_RELEASES_SPINNER,
    MSG_FETCH_SOLC_RELEASES_SPINNER, MSG_GET_SOLC_RELEASES_ERR, MSG_GET_VYPER_RELEASES_ERR,
    MSG_GET_ZKSOLC_RELEASES_ERR, MSG_GET_ZKVYPER_RELEASES_ERR, MSG_NO_VERSION_FOUND_ERR,
    MSG_OS_NOT_SUPPORTED_ERR, MSG_SOLC_VERSION_PROMPT, MSG_VYPER_VERSION_PROMPT,
    MSG_ZKSOLC_VERSION_PROMPT, MSG_ZKVYPER_VERSION_PROMPT,
};

#[derive(Debug, Clone, Parser, Default)]
pub struct InitContractVerifierArgs {
    /// Version of zksolc to install
    #[clap(long)]
    pub zksolc_version: Option<String>,
    /// Version of zkvyper to install
    #[clap(long)]
    pub zkvyper_version: Option<String>,
    /// Version of solc to install
    #[clap(long)]
    pub solc_version: Option<String>,
    /// Version of vyper to install
    #[clap(long)]
    pub vyper_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InitContractVerifierArgsFinal {
    pub zksolc_releases: Vec<Version>,
    pub zkvyper_releases: Vec<Version>,
    pub solc_releases: Vec<Version>,
    pub vyper_releases: Vec<Version>,
}

impl InitContractVerifierArgs {
    pub fn fill_values_with_prompt(
        self,
        shell: &Shell,
    ) -> anyhow::Result<InitContractVerifierArgsFinal> {
        let arch = get_arch()?;

        let zksolc_releases = get_releases_with_arch(
            shell,
            "matter-labs/zksolc-bin",
            arch,
            MSG_FETCHING_ZKSOLC_RELEASES_SPINNER,
        )
        .context(MSG_GET_ZKSOLC_RELEASES_ERR)?;

        let zkvyper_releases = get_releases_with_arch(
            shell,
            "matter-labs/zkvyper-bin",
            arch,
            MSG_FETCHING_ZKVYPER_RELEASES_SPINNER,
        )
        .context(MSG_GET_ZKVYPER_RELEASES_ERR)?;

        let solc_releases = get_releases_with_arch(
            shell,
            "ethereum/solc-bin",
            arch,
            MSG_FETCH_SOLC_RELEASES_SPINNER,
        )
        .context(MSG_GET_SOLC_RELEASES_ERR)?;

        let vyper_releases = get_releases_with_arch(
            shell,
            "vyperlang/vyper",
            arch,
            MSG_FETCHING_VYPER_RELEASES_SPINNER,
        )
        .context(MSG_GET_VYPER_RELEASES_ERR)?;

        let zksolc_version = select_min_version(
            self.zksolc_version,
            zksolc_releases.clone(),
            MSG_ZKSOLC_VERSION_PROMPT,
        )?;
        let zksolc_releases = get_releases_above_version(zksolc_releases, zksolc_version)?;

        let zkvyper_version = select_min_version(
            self.zkvyper_version,
            zkvyper_releases.clone(),
            MSG_ZKVYPER_VERSION_PROMPT,
        )?;
        let zkvyper_releases = get_releases_above_version(zkvyper_releases, zkvyper_version)?;

        let solc_version = select_min_version(
            self.solc_version,
            solc_releases.clone(),
            MSG_SOLC_VERSION_PROMPT,
        )?;
        let solc_releases = get_releases_above_version(solc_releases, solc_version)?;

        let vyper_version = select_min_version(
            self.vyper_version,
            vyper_releases.clone(),
            MSG_VYPER_VERSION_PROMPT,
        )?;
        let vyper_releases = get_releases_above_version(vyper_releases, vyper_version)?;

        Ok(InitContractVerifierArgsFinal {
            zksolc_releases,
            zkvyper_releases,
            solc_releases,
            vyper_releases,
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

fn select_min_version(
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

fn get_releases_above_version(
    releases: Vec<Version>,
    version: Version,
) -> anyhow::Result<Vec<Version>> {
    let pos = releases
        .iter()
        .position(|r| r.version == version.version)
        .context(MSG_NO_VERSION_FOUND_ERR)?;

    Ok(releases[..=pos].to_vec())
}
