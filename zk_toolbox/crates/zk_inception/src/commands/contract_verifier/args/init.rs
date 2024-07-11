use anyhow::Context;
use clap::Parser;
use common::{spinner::Spinner, PromptSelect};
use xshell::Shell;

use crate::messages::{
    MSG_ARCH_NOT_SUPPORTED_ERR, MSG_FETCHING_ZKSOLC_RELEASES_SPINNER, MSG_GET_ZKSOLC_RELEASES_ERR,
    MSG_NO_RELEASES_FOUND_ERR, MSG_NO_VERSION_FOUND_ERR, MSG_OS_NOT_SUPPORTED_ERR,
    MSG_ZKSOLC_VERSION_PROMPT,
};

use super::zksolc_releases::{get_zksolc_releases, Arch, ZkSolcVersion};

#[derive(Debug, Clone, Parser, Default)]
pub struct InitContractVerifierArgs {
    /// Version of zksolc to install
    #[clap(long)]
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InitContractVerifierArgsFinal {
    pub zksolc_version: ZkSolcVersion,
}

impl InitContractVerifierArgs {
    pub fn fill_values_with_prompt(
        self,
        shell: &Shell,
    ) -> anyhow::Result<InitContractVerifierArgsFinal> {
        let spinner = Spinner::new(MSG_FETCHING_ZKSOLC_RELEASES_SPINNER);
        let releases = get_zksolc_releases(shell).context(MSG_GET_ZKSOLC_RELEASES_ERR)?;
        spinner.finish();

        let arch = get_arch()?;

        let releases = releases
            .into_iter()
            .filter(|r| r.arch == arch)
            .collect::<Vec<_>>();

        if releases.is_empty() {
            anyhow::bail!(MSG_NO_RELEASES_FOUND_ERR);
        }

        let version = self.version.unwrap_or_else(|| {
            PromptSelect::new(
                MSG_ZKSOLC_VERSION_PROMPT,
                releases.iter().map(|r| &r.version),
            )
            .ask()
            .into()
        });

        let zksolc_version = releases
            .iter()
            .find(|r| r.version == version)
            .context(MSG_NO_VERSION_FOUND_ERR)?
            .to_owned();

        Ok(InitContractVerifierArgsFinal { zksolc_version })
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
