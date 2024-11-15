use std::collections::HashMap;

use anyhow::Context as _;
use tokio::io::AsyncWriteExt as _;
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
    /// Holds versions for both upstream and zkVM solc.
    solc_versions: HashMap<String, reqwest::Url>,
    zksolc_versions: HashMap<String, reqwest::Url>,
    vyper_versions: HashMap<String, reqwest::Url>,
    zkvyper_versions: HashMap<String, reqwest::Url>,
}

impl GitHubCompilerResolver {
    pub async fn new() -> anyhow::Result<Self> {
        let artifacts_dir = tempfile::tempdir().context("failed creating temp dir")?;
        let gh_client = GitHubApi::new();

        let mut self_ = Self {
            artifacts_dir,
            gh_client,
            client: reqwest::Client::new(),
            solc_versions: HashMap::new(),
            zksolc_versions: HashMap::new(),
            vyper_versions: HashMap::new(),
            zkvyper_versions: HashMap::new(),
        };
        if let Err(err) = self_.sync_compiler_versions().await {
            // We don't want the resolver to fail at creation if versions can't be fetched.
            // It shouldn't bring down the whole application, so the expectation here is that
            // the versions will be fetched later.
            tracing::error!("failed syncing compiler versions at start: {:?}", err);
        }

        Ok(self_)
    }
}

impl GitHubCompilerResolver {
    async fn sync_compiler_versions(&mut self) -> anyhow::Result<()> {
        self.solc_versions = self
            .gh_client
            .solc_versions()
            .await
            .context("failed fetching solc versions")?;
        self.zksolc_versions = self
            .gh_client
            .zksolc_versions()
            .await
            .context("failed fetching zksolc versions")?;
        self.vyper_versions = self
            .gh_client
            .vyper_versions()
            .await
            .context("failed fetching vyper versions")?;
        self.zkvyper_versions = self
            .gh_client
            .zkvyper_versions()
            .await
            .context("failed fetching zkvyper versions")?;
        Ok(())
    }

    async fn download_version_if_needed(
        &self,
        compiler: CompilerType,
        version: &str,
    ) -> anyhow::Result<()> {
        if compiler.exists(self.artifacts_dir.path(), version).await? {
            tracing::debug!("Compiler {}:{} exists", compiler.as_str(), version);
            return Ok(());
        }

        tracing::info!("Downloading {}:{}", compiler.as_str(), version);
        let versions = match compiler {
            CompilerType::Solc => &self.solc_versions,
            CompilerType::ZkSolc => &self.zksolc_versions,
            CompilerType::Vyper => &self.vyper_versions,
            CompilerType::ZkVyper => &self.zkvyper_versions,
        };

        let version_url = versions
            .get(version)
            .ok_or_else(|| {
                ContractVerifierError::UnknownCompilerVersion("solc", version.to_owned())
            })?
            .clone();
        let path = compiler.bin_path_unchecked(self.artifacts_dir.path(), version);

        let response = self.client.get(version_url).send().await?;
        let body = response.bytes().await?;

        tracing::info!("Saving {}:{} to {:?}", compiler.as_str(), version, path);

        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .map_err(|e| anyhow::anyhow!("failed to create dir: {}", e))?;

        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create file: {}", e))?;
        file.write_all(&body)
            .await
            .map_err(|e| anyhow::anyhow!("failed to write to file: {}", e))?;
        file.flush()
            .await
            .map_err(|e| anyhow::anyhow!("failed to flush file: {}", e))?;

        // On UNIX-like systems, make file executable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = file.metadata().await?.permissions();
            perms.set_mode(0o700); // Only owner can execute and access.
            file.set_permissions(perms).await?;
        }

        tracing::info!("Finished downloading {}:{}", compiler.as_str(), version);

        Ok(())
    }
}

#[async_trait]
impl CompilerResolver for GitHubCompilerResolver {
    async fn supported_versions(&self) -> anyhow::Result<SupportedCompilerVersions> {
        let versions = SupportedCompilerVersions {
            solc: self.solc_versions.keys().cloned().collect(),
            zksolc: self.zksolc_versions.keys().cloned().collect(),
            vyper: self.vyper_versions.keys().cloned().collect(),
            zkvyper: self.zkvyper_versions.keys().cloned().collect(),
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
