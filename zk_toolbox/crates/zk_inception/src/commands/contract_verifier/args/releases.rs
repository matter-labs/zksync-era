use std::str::FromStr;

use common::{cmd::Cmd, spinner::Spinner};
use serde::Deserialize;
use xshell::{cmd, Shell};

use crate::messages::{MSG_INVALID_ARCH_ERR, MSG_NO_RELEASES_FOUND_ERR};

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub version: String,
    pub arch: Vec<Arch>,
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

fn get_compatible_archs(asset_name: &str) -> anyhow::Result<Vec<Arch>> {
    if let Ok(arch) = Arch::from_str(asset_name) {
        Ok(vec![arch])
    } else {
        if asset_name.contains(".linux") {
            Ok(vec![Arch::LinuxAmd, Arch::LinuxArm])
        } else if asset_name.contains(".darwin") {
            Ok(vec![Arch::MacosAmd, Arch::MacosArm])
        } else {
            Err(anyhow::anyhow!(MSG_INVALID_ARCH_ERR))
        }
    }
}

fn get_releases(shell: &Shell, repo: &str) -> anyhow::Result<Vec<Version>> {
    let response: std::process::Output = Cmd::new(cmd!(
        shell,
        "curl https://api.github.com/repos/{repo}/releases"
    ))
    .run_with_output()?;

    let response = String::from_utf8(response.stdout)?;
    let releases: Vec<GitHubRelease> = serde_json::from_str(&response)?;

    let mut versions = vec![];

    for release in releases {
        let version = release.tag_name;
        for asset in release.assets {
            let arch = match get_compatible_archs(&asset.name) {
                Ok(arch) => arch,
                Err(_) => continue,
            };
            let url = asset.browser_download_url;
            versions.push(Version {
                version: version.clone(),
                arch,
                url,
            });
        }
    }

    Ok(versions)
}

pub fn get_releases_with_arch(
    shell: &Shell,
    repo: &str,
    arch: Arch,
    message: &str,
) -> anyhow::Result<Vec<Version>> {
    let spinner = Spinner::new(message);
    let releases = get_releases(shell, repo)?;
    let releases = releases
        .into_iter()
        .filter(|r| r.arch.contains(&arch))
        .collect::<Vec<_>>();
    if releases.is_empty() {
        anyhow::bail!(MSG_NO_RELEASES_FOUND_ERR);
    }
    spinner.finish();
    Ok(releases)
}
