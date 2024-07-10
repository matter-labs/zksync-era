use std::str::FromStr;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use common::{cmd::Cmd, spinner::Spinner, PromptSelect};
use strum::IntoEnumIterator;
use xshell::{cmd, Shell};

use crate::messages::{
    MSG_ARCH_SLECTION_PROMPT, MSG_FETCHING_ZKSOLC_RELEASES_SPINNER, MSG_GET_ZKSOLC_RELEASES_ERR,
    MSG_INVALID_ARCH_ERR, MSG_NO_RELEASES_FOUND_ERR, MSG_NO_VERSION_FOUND_ERR,
    MSG_ZKSOLC_VERSION_PROMPT,
};

#[derive(Debug, Clone, Parser, Default)]
pub struct InitContractVerifierArgs {
    /// Version of zksolc to install
    #[clap(long)]
    pub version: Option<String>,
    /// Architecture of zksolc to install
    #[clap(long)]
    pub arch: Option<Arch>,
}

#[derive(Debug, Clone)]
pub struct InitContractVerifierArgsFinal {
    pub zksolc_version: ZkSolcVersion,
}

#[derive(Debug, Clone)]
pub struct ZkSolcVersion {
    pub version: String,
    pub arch: Arch,
    pub url: String,
}

#[derive(Debug, Clone, ValueEnum, strum::EnumIter, PartialEq, Eq, Copy)]
pub enum Arch {
    LinuxAmd,
    LinuxArm,
    MacosAmd,
    MacosArm,
    WindowsAmd,
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arch::LinuxAmd => write!(f, "linux-amd64"),
            Arch::LinuxArm => write!(f, "linux-arm64"),
            Arch::MacosAmd => write!(f, "macos-amd64"),
            Arch::MacosArm => write!(f, "macos-arm64"),
            Arch::WindowsAmd => write!(f, "windows-amd64"),
        }
    }
}

impl std::str::FromStr for Arch {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains("linux-amd64") {
            Ok(Arch::LinuxAmd)
        } else if s.contains("linux-arm64") {
            Ok(Arch::LinuxArm)
        } else if s.contains("macosx-amd64") {
            Ok(Arch::MacosAmd)
        } else if s.contains("macosx-arm64") {
            Ok(Arch::MacosArm)
        } else if s.contains("windows-amd64") {
            Ok(Arch::WindowsAmd)
        } else {
            Err(anyhow::anyhow!(MSG_INVALID_ARCH_ERR))
        }
    }
}

impl InitContractVerifierArgs {
    pub fn fill_values_with_prompt(
        self,
        shell: &Shell,
    ) -> anyhow::Result<InitContractVerifierArgsFinal> {
        let spinner = Spinner::new(MSG_FETCHING_ZKSOLC_RELEASES_SPINNER);
        let releases = get_zksolc_releases(shell)?;
        spinner.finish();

        let arch = self
            .arch
            .unwrap_or_else(|| PromptSelect::new(MSG_ARCH_SLECTION_PROMPT, Arch::iter()).ask());

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

fn get_zksolc_releases(shell: &Shell) -> anyhow::Result<Vec<ZkSolcVersion>> {
    let response: std::process::Output = Cmd::new(cmd!(
        shell,
        "curl https://api.github.com/repos/matter-labs/zksolc-bin/releases"
    ))
    .run_with_output()?;
    let response = String::from_utf8(response.stdout)?;
    let response: serde_json::Value = serde_json::from_str(&response)?;

    let mut releases = vec![];

    for r in response.as_array().context(MSG_GET_ZKSOLC_RELEASES_ERR)? {
        let version = r["name"]
            .as_str()
            .context(MSG_GET_ZKSOLC_RELEASES_ERR)?
            .to_string();
        let assets = r["assets"]
            .as_array()
            .context(MSG_GET_ZKSOLC_RELEASES_ERR)?;
        for a in assets.iter() {
            let arch = <Arch as FromStr>::from_str(
                a["name"].as_str().context(MSG_GET_ZKSOLC_RELEASES_ERR)?,
            )?;
            let url = a["browser_download_url"]
                .as_str()
                .context(MSG_GET_ZKSOLC_RELEASES_ERR)?
                .to_string();
            releases.push(ZkSolcVersion {
                version: version.clone(),
                arch,
                url,
            });
        }
    }

    Ok(releases)
}
