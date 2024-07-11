use std::str::FromStr;

use common::cmd::Cmd;
use serde::Deserialize;
use xshell::{cmd, Shell};

use crate::messages::MSG_INVALID_ARCH_ERR;

#[derive(Deserialize)]
struct GitHubRelease {
    name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkSolcVersion {
    pub version: String,
    pub arch: Arch,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum Arch {
    LinuxAmd,
    LinuxArm,
    MacosAmd,
    MacosArm,
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
        } else {
            Err(anyhow::anyhow!(MSG_INVALID_ARCH_ERR))
        }
    }
}

pub fn get_zksolc_releases(shell: &Shell) -> anyhow::Result<Vec<ZkSolcVersion>> {
    let response: std::process::Output = Cmd::new(cmd!(
        shell,
        "curl https://api.github.com/repos/matter-labs/zksolc-bin/releases"
    ))
    .run_with_output()?;

    let response = String::from_utf8(response.stdout)?;
    let releases: Vec<GitHubRelease> = serde_json::from_str(&response)?;

    let mut zk_solc_versions = vec![];

    for release in releases {
        let version = release.name;
        for asset in release.assets {
            let arch = match Arch::from_str(&asset.name) {
                Ok(arch) => arch,
                Err(_) => continue,
            };
            let url = asset.browser_download_url;
            zk_solc_versions.push(ZkSolcVersion {
                version: version.clone(),
                arch,
                url,
            });
        }
    }

    Ok(zk_solc_versions)
}
