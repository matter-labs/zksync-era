//! A thin wrapper over the GitHub API for the purposes of the contract verifier.

use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context as _;
use futures_util::TryStreamExt as _;
use octocrab::service::middleware::retry::RetryConfig;

/// Representation of releases of the compiler.
/// The main difference from the `CompilerType` used in the `resolver` module is that
/// we treat `ZkVmSolc` differently, as it's stored in a different repository.
#[derive(Debug, Clone, Copy)]
pub(super) enum CompilerGitHubRelease {
    /// "Upstream" Solidity
    Solc,
    /// "Upstream" Vyper
    Vyper,
    /// ZkSync's fork of the Solidity compiler
    /// Used as a dependency for ZkSolc
    ZkVmSolc,
    /// Solidity compiler for EraVM
    ZkSolc,
    /// Vyper compiler for EraVM
    ZkVyper,
}

impl CompilerGitHubRelease {
    fn organization(self) -> &'static str {
        match self {
            Self::Solc => "ethereum",
            Self::Vyper => "vyperlang",
            Self::ZkVmSolc => "matter-labs",
            Self::ZkSolc => "matter-labs",
            Self::ZkVyper => "matter-labs",
        }
    }

    fn repo(self) -> &'static str {
        match self {
            Self::Solc => "solidity",
            Self::Vyper => "vyper",
            Self::ZkVmSolc => "era-solidity",
            Self::ZkSolc => "era-compiler-solidity",
            Self::ZkVyper => "era-compiler-vyper",
        }
    }

    /// Check if version is blacklisted, e.g. it shouldn't be available in the contract verifier.
    fn is_version_blacklisted(self, version: &str) -> bool {
        match self {
            Self::Solc => {
                let Ok(version) = semver::Version::parse(version) else {
                    tracing::error!(
                        "Incorrect version passed to blacklist check: {self:?}:{version}"
                    );
                    return true;
                };
                // The earliest supported version is 0.4.10.
                version < semver::Version::new(0, 4, 10)
            }
            Self::Vyper => {
                let Ok(version) = semver::Version::parse(version) else {
                    tracing::error!(
                        "Incorrect version passed to blacklist check: {self:?}:{version}"
                    );
                    return true;
                };

                // Versions below `0.3` are not supported.
                if version < semver::Version::new(0, 3, 0) {
                    return true;
                }

                // In `0.3.x` we only allow `0.3.3`, `0.3.9`, and `0.3.10`.
                if version.minor == 3 {
                    return !matches!(version.patch, 3 | 9 | 10);
                }

                false
            }
            _ => false,
        }
    }

    fn extract_version(self, tag_name: &str) -> Option<String> {
        match self {
            Self::Solc | Self::Vyper => {
                // Solidity and Vyper releases are tagged with version numbers in form of `vX.Y.Z`.
                // Our API does not require the `v` prefix for solc/vyper, so we strip it.
                tag_name
                    .strip_prefix('v')
                    .filter(|v| semver::Version::parse(v).is_ok())
                    .map(|v| v.to_string())
            }
            Self::ZkVmSolc => {
                // ZkVmSolc releases are tagged with version numbers in form of `X.Y.Z-A.B.C`, where
                // `X.Y.Z` is the version of the Solidity compiler, and `A.B.C` is the version of the ZkSync fork.
                // `v` prefix is not required.
                if let Some((main, fork)) = tag_name.split_once('-') {
                    if semver::Version::parse(main).is_ok() && semver::Version::parse(fork).is_ok()
                    {
                        // In contract verifier, our fork is prefixed with `zkVM-`.
                        return Some(format!("zkVM-{tag_name}"));
                    }
                }
                None
            }
            Self::ZkSolc | Self::ZkVyper => {
                // zksolc and zkvyper releases are tagged with version numbers in form of `X.Y.Z` (without 'v').
                // Our API expects versions to be prefixed with `v` for zksolc/zkvyper, so we add it.
                if semver::Version::parse(tag_name).is_ok() {
                    Some(format!("v{tag_name}"))
                } else {
                    None
                }
            }
        }
    }

    fn match_asset(&self, asset_url: &str) -> bool {
        match self {
            Self::Solc => asset_url.contains("solc-static-linux"),
            Self::Vyper => asset_url.contains(".linux"),
            Self::ZkVmSolc => asset_url.contains("solc-linux-amd64"),
            Self::ZkSolc => asset_url.contains("zksolc-linux-amd64-musl"),
            Self::ZkVyper => asset_url.contains("zkvyper-linux-amd64-musl"),
        }
    }
}

/// A thin wrapper over the GitHub API for the purposes of the contract verifier.
#[derive(Debug)]
pub(super) struct GitHubApi {
    client: Arc<octocrab::Octocrab>,
}

impl GitHubApi {
    /// Creates a new instance of the GitHub API wrapper.
    pub(super) fn new() -> Self {
        // Octocrab requires rustls to be configured.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        let client = Arc::new(
            octocrab::Octocrab::builder()
                .add_retry_config(Self::retry_config())
                .set_connect_timeout(Some(Self::connect_timeout()))
                .set_read_timeout(Some(Self::read_timeout()))
                .build()
                .unwrap(),
        );
        Self { client }
    }

    fn retry_config() -> RetryConfig {
        RetryConfig::Simple(4)
    }

    fn connect_timeout() -> Duration {
        Duration::from_secs(10)
    }

    fn read_timeout() -> Duration {
        Duration::from_secs(60)
    }

    /// Returns versions for both upstream and our fork of solc.
    pub async fn solc_versions(&self) -> anyhow::Result<HashMap<String, reqwest::Url>> {
        let mut versions = self
            .extract_versions(CompilerGitHubRelease::Solc)
            .await
            .context("Can't fetch upstream solc versions")?;
        versions.extend(
            self.extract_versions(CompilerGitHubRelease::ZkVmSolc)
                .await
                .context("Can't fetch zkVM solc versions")?,
        );
        Ok(versions)
    }

    pub async fn zksolc_versions(&self) -> anyhow::Result<HashMap<String, reqwest::Url>> {
        self.extract_versions(CompilerGitHubRelease::ZkSolc).await
    }

    pub async fn vyper_versions(&self) -> anyhow::Result<HashMap<String, reqwest::Url>> {
        self.extract_versions(CompilerGitHubRelease::Vyper).await
    }

    pub async fn zkvyper_versions(&self) -> anyhow::Result<HashMap<String, reqwest::Url>> {
        self.extract_versions(CompilerGitHubRelease::ZkVyper).await
    }

    /// Will scan all the releases for a specific compiler.
    async fn extract_versions(
        &self,
        compiler: CompilerGitHubRelease,
    ) -> anyhow::Result<HashMap<String, reqwest::Url>> {
        // Create a stream over all the versions to not worry about pagination.
        let releases = self
            .client
            .repos(compiler.organization(), compiler.repo())
            .releases()
            .list()
            .per_page(100)
            .send()
            .await?
            .into_stream(&self.client);
        tokio::pin!(releases);

        // Go through all the releases, filter ones that match the version.
        // For matching versions, find a suitable asset and store its URL.
        let mut versions = HashMap::new();
        while let Some(release) = releases.try_next().await? {
            // Skip pre-releases.
            if release.prerelease {
                continue;
            }

            if let Some(version) = compiler.extract_version(&release.tag_name) {
                if compiler.is_version_blacklisted(&version) {
                    tracing::debug!("Skipping {compiler:?}:{version} due to blacklist");
                    continue;
                }

                let mut found = false;
                for asset in release.assets {
                    if compiler.match_asset(asset.browser_download_url.as_str()) {
                        tracing::info!("Discovered release {compiler:?}:{version}");
                        versions.insert(version.clone(), asset.browser_download_url.clone());
                        found = true;
                        break;
                    }
                }
                if !found {
                    tracing::warn!("Didn't find a matching artifact for {compiler:?}:{version}");
                }
            }
        }

        Ok(versions)
    }
}
