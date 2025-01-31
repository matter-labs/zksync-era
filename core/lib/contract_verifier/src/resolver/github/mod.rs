use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use tokio::{io::AsyncWriteExt as _, sync::RwLock};
use zksync_queued_job_processor::async_trait;

use self::gh_api::GitHubApi;
use crate::{
    compilers::{Solc, SolcInput, Vyper, VyperInput, ZkSolc, ZkSolcInput, ZkVyper},
    error::ContractVerifierError,
    resolver::{
        Compiler, CompilerPaths, CompilerResolver, CompilerType, SupportedCompilerVersions,
    },
    ZkCompilerVersions,
};

mod gh_api;

/// [`CompilerResolver`] that can dynamically download missing compilers from GitHub releases.
///
/// Note: this resolver does not interact with [`EnvCompilerResolver`](super::EnvCompilerResolver).
/// This is important for the context of zksolc/zkvyper, as there we have two separate compilers
/// required for compilation. This resolver will download both of them, even if one of the versions
/// is available in the `EnvCompilerResolver`.
#[derive(Debug)]
pub(crate) struct GitHubCompilerResolver {
    /// We expect that contract-verifier will be running in docker without any persistent storage,
    /// so we explicitly don't expect any artifacts to survive restart.
    artifacts_dir: tempfile::TempDir,
    gh_client: GitHubApi,
    client: reqwest::Client,
    supported_versions: RwLock<SupportedVersions>,
    /// List of downloads performed right now.
    /// `broadcast` receiver can be used to wait until the download is finished.
    active_downloads: RwLock<HashMap<(CompilerType, String), tokio::sync::broadcast::Receiver<()>>>,
}

#[derive(Debug)]
struct SupportedVersions {
    /// Holds versions for both upstream and zkVM solc.
    solc_versions: HashMap<String, reqwest::Url>,
    zksolc_versions: HashMap<String, reqwest::Url>,
    vyper_versions: HashMap<String, reqwest::Url>,
    zkvyper_versions: HashMap<String, reqwest::Url>,
    last_updated: Instant,
}

impl Default for SupportedVersions {
    fn default() -> Self {
        Self::new()
    }
}

impl SupportedVersions {
    // Note: We assume that contract verifier will run the task to update supported versions
    // rarely, but we still want to protect ourselves from accidentally spamming GitHub API.
    // So, this interval is smaller than the expected time between updates (this way we don't
    // run into an issue where intervals are slightly out of sync, causing a delay in "real"
    // update time).
    const CACHE_INTERVAL: Duration = Duration::from_secs(10 * 60); // 10 minutes

    fn new() -> Self {
        Self {
            solc_versions: HashMap::new(),
            zksolc_versions: HashMap::new(),
            vyper_versions: HashMap::new(),
            zkvyper_versions: HashMap::new(),
            last_updated: Instant::now(),
        }
    }

    fn is_outdated(&self) -> bool {
        self.last_updated.elapsed() > Self::CACHE_INTERVAL
    }

    async fn update(&mut self, gh_client: &GitHubApi) -> anyhow::Result<()> {
        // Non-atomic update is fine here: the fields are independent, so if
        // at least one update succeeds, it's worth persisting. We won't be changing
        // the last update timestamp in case of failure though, so it will be retried
        // next time.
        self.solc_versions = gh_client
            .solc_versions()
            .await
            .context("failed fetching solc versions")?;
        self.zksolc_versions = gh_client
            .zksolc_versions()
            .await
            .context("failed fetching zksolc versions")?;
        self.vyper_versions = gh_client
            .vyper_versions()
            .await
            .context("failed fetching vyper versions")?;
        self.zkvyper_versions = gh_client
            .zkvyper_versions()
            .await
            .context("failed fetching zkvyper versions")?;
        self.last_updated = Instant::now();
        Ok(())
    }

    async fn update_if_needed(&mut self, gh_client: &GitHubApi) -> anyhow::Result<()> {
        if self.is_outdated() {
            tracing::info!("GH compiler versions cache outdated, updating");
            self.update(gh_client).await?;
        }
        Ok(())
    }
}

impl GitHubCompilerResolver {
    pub async fn new() -> anyhow::Result<Self> {
        let artifacts_dir = tempfile::tempdir().context("failed creating temp dir")?;
        let gh_client = GitHubApi::new();
        let mut supported_versions = SupportedVersions::default();
        if let Err(err) = supported_versions.update(&gh_client).await {
            // We don't want the resolver to fail at creation if versions can't be fetched.
            // It shouldn't bring down the whole application, so the expectation here is that
            // the versions will be fetched later.
            tracing::error!("failed syncing compiler versions at start: {:?}", err);
        }

        Ok(Self {
            artifacts_dir,
            gh_client,
            client: reqwest::Client::new(),
            supported_versions: RwLock::new(supported_versions),
            active_downloads: RwLock::default(),
        })
    }
}

impl GitHubCompilerResolver {
    async fn download_version_if_needed(
        &self,
        compiler: CompilerType,
        version: &str,
    ) -> anyhow::Result<()> {
        // We need to check the lock first, because the compiler may still be downloading.
        // We must hold the lock until we know if we need to download the compiler.
        let mut lock = self.active_downloads.write().await;
        if let Some(rx) = lock.get(&(compiler, version.to_string())) {
            let mut rx = rx.resubscribe();
            drop(lock);
            tracing::debug!(
                "Waiting for {}:{} download to finish",
                compiler.as_str(),
                version
            );
            rx.recv().await?;
            return Ok(());
        }

        if compiler.exists(self.artifacts_dir.path(), version).await? {
            tracing::debug!("Compiler {}:{} exists", compiler.as_str(), version);
            return Ok(());
        }

        // Mark the compiler as downloading.
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        lock.insert((compiler, version.to_string()), rx);
        drop(lock);

        tracing::info!("Downloading {}:{}", compiler.as_str(), version);
        let lock = self.supported_versions.read().await;
        let versions = match compiler {
            CompilerType::Solc => &lock.solc_versions,
            CompilerType::ZkSolc => &lock.zksolc_versions,
            CompilerType::Vyper => &lock.vyper_versions,
            CompilerType::ZkVyper => &lock.zkvyper_versions,
        };

        let version_url = versions
            .get(version)
            .ok_or_else(|| {
                ContractVerifierError::UnknownCompilerVersion("solc", version.to_owned())
            })?
            .clone();
        drop(lock);
        let path = compiler.bin_path_unchecked(self.artifacts_dir.path(), version);

        let response = self.client.get(version_url).send().await?;
        let body = response.bytes().await?;

        tracing::info!("Saving {}:{} to {:?}", compiler.as_str(), version, path);

        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .context("failed to create dir")?;

        let mut file = tokio::fs::File::create_new(path)
            .await
            .context("failed to create file")?;
        file.write_all(&body)
            .await
            .context("failed to write to file")?;
        file.flush().await.context("failed to flush file")?;

        // On UNIX-like systems, make file executable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = file.metadata().await?.permissions();
            perms.set_mode(0o700); // Only owner can execute and access.
            file.set_permissions(perms).await?;
        }

        tracing::info!("Finished downloading {}:{}", compiler.as_str(), version);

        // Notify other waiters that the compiler is downloaded.
        tx.send(()).ok();
        let mut lock = self.active_downloads.write().await;
        lock.remove(&(compiler, version.to_string()));
        drop(lock);

        Ok(())
    }
}

#[async_trait]
impl CompilerResolver for GitHubCompilerResolver {
    async fn supported_versions(&self) -> anyhow::Result<SupportedCompilerVersions> {
        let mut lock = self.supported_versions.write().await;
        lock.update_if_needed(&self.gh_client).await?;

        let versions = SupportedCompilerVersions {
            solc: lock.solc_versions.keys().cloned().collect(),
            zksolc: lock.zksolc_versions.keys().cloned().collect(),
            vyper: lock.vyper_versions.keys().cloned().collect(),
            zkvyper: lock.zkvyper_versions.keys().cloned().collect(),
        };
        tracing::info!("GitHubResolver supported versions: {:?}", versions);
        Ok(versions)
    }

    async fn resolve_solc(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<SolcInput>>, ContractVerifierError> {
        self.download_version_if_needed(CompilerType::Solc, version)
            .await?;

        let solc_path = CompilerType::Solc
            .bin_path(self.artifacts_dir.path(), version)
            .await?;
        Ok(Box::new(Solc::new(solc_path)))
    }

    async fn resolve_zksolc(
        &self,
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkSolcInput>>, ContractVerifierError> {
        self.download_version_if_needed(CompilerType::Solc, &version.base)
            .await?;
        self.download_version_if_needed(CompilerType::ZkSolc, &version.zk)
            .await?;

        let zksolc_version = &version.zk;
        let zksolc_path = CompilerType::ZkSolc
            .bin_path(self.artifacts_dir.path(), zksolc_version)
            .await?;
        let solc_path = CompilerType::Solc
            .bin_path(self.artifacts_dir.path(), &version.base)
            .await?;
        let compiler_paths = CompilerPaths {
            base: solc_path,
            zk: zksolc_path,
        };
        Ok(Box::new(ZkSolc::new(
            compiler_paths,
            zksolc_version.to_owned(),
        )))
    }

    async fn resolve_vyper(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError> {
        self.download_version_if_needed(CompilerType::Vyper, version)
            .await?;

        let vyper_path = CompilerType::Vyper
            .bin_path(self.artifacts_dir.path(), version)
            .await?;
        Ok(Box::new(Vyper::new(vyper_path)))
    }

    async fn resolve_zkvyper(
        &self,
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError> {
        self.download_version_if_needed(CompilerType::Vyper, &version.base)
            .await?;
        self.download_version_if_needed(CompilerType::ZkVyper, &version.zk)
            .await?;

        let zkvyper_path = CompilerType::ZkVyper
            .bin_path(self.artifacts_dir.path(), &version.zk)
            .await?;
        let vyper_path = CompilerType::Vyper
            .bin_path(self.artifacts_dir.path(), &version.base)
            .await?;
        let compiler_paths = CompilerPaths {
            base: vyper_path,
            zk: zkvyper_path,
        };
        Ok(Box::new(ZkVyper::new(compiler_paths)))
    }
}
