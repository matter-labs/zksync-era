use std::{
    fmt,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use tokio::fs;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification_api::{CompilationArtifacts, CompilerVersions};
use zksync_utils::env::Workspace;

use crate::{
    compilers::{Solc, SolcInput, ZkSolc, ZkSolcInput, ZkVyper, ZkVyperInput},
    error::ContractVerifierError,
};

#[derive(Debug, Clone, Copy)]
enum CompilerType {
    Solc,
    ZkSolc,
    Vyper,
    ZkVyper,
}

impl CompilerType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Solc => "solc",
            Self::ZkSolc => "zksolc",
            Self::Vyper => "vyper",
            Self::ZkVyper => "zkvyper",
        }
    }

    /// Returns the absolute path to the compiler binary.
    fn bin_path_unchecked(self, home_dir: &Path, version: &str) -> PathBuf {
        let compiler_dir = match self {
            Self::Solc => "solc-bin",
            Self::ZkSolc => "zksolc-bin",
            Self::Vyper => "vyper-bin",
            Self::ZkVyper => "zkvyper-bin",
        };
        home_dir
            .join("etc")
            .join(compiler_dir)
            .join(version)
            .join(self.as_str())
    }

    async fn bin_path(
        self,
        home_dir: &Path,
        version: &str,
    ) -> Result<PathBuf, ContractVerifierError> {
        let path = self.bin_path_unchecked(home_dir, version);
        if !fs::try_exists(&path)
            .await
            .with_context(|| format!("failed accessing `{}`", self.as_str()))?
        {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                self.as_str(),
                version.to_owned(),
            ));
        }
        Ok(path)
    }
}

/// Compiler versions supported by a [`CompilerResolver`].
#[derive(Debug)]
pub(crate) struct SupportedCompilerVersions {
    pub solc: Vec<String>,
    pub zksolc: Vec<String>,
    pub vyper: Vec<String>,
    pub zkvyper: Vec<String>,
}

impl SupportedCompilerVersions {
    pub fn lacks_any_compiler(&self) -> bool {
        self.solc.is_empty()
            || self.zksolc.is_empty()
            || self.vyper.is_empty()
            || self.zkvyper.is_empty()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompilerPaths {
    /// Path to the base (non-zk) compiler.
    pub base: PathBuf,
    /// Path to the zk compiler.
    pub zk: PathBuf,
}

/// Encapsulates compiler paths resolution.
#[async_trait]
pub(crate) trait CompilerResolver: fmt::Debug + Send + Sync {
    /// Returns compiler versions supported by this resolver.
    ///
    /// # Errors
    ///
    /// Returned errors are assumed to be fatal.
    async fn supported_versions(&self) -> anyhow::Result<SupportedCompilerVersions>;

    /// Resolves a `solc` compiler.
    async fn resolve_solc(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<SolcInput>>, ContractVerifierError>;

    /// Resolves a `zksolc` compiler.
    async fn resolve_zksolc(
        &self,
        versions: &CompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkSolcInput>>, ContractVerifierError>;

    /// Resolves a `zkvyper` compiler.
    async fn resolve_zkvyper(
        &self,
        versions: &CompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkVyperInput>>, ContractVerifierError>;
}

/// Encapsulates a one-off compilation process.
#[async_trait]
pub(crate) trait Compiler<In>: Send + fmt::Debug {
    /// Performs compilation.
    async fn compile(
        self: Box<Self>,
        input: In,
    ) -> Result<CompilationArtifacts, ContractVerifierError>;
}

/// Default [`CompilerResolver`] using pre-downloaded compilers in the `/etc` subdirectories (relative to the workspace).
#[derive(Debug)]
pub(crate) struct EnvCompilerResolver {
    home_dir: PathBuf,
}

impl Default for EnvCompilerResolver {
    fn default() -> Self {
        Self {
            home_dir: Workspace::locate().core(),
        }
    }
}

impl EnvCompilerResolver {
    async fn read_dir(&self, dir: &str) -> anyhow::Result<Vec<String>> {
        let mut dir_entries = fs::read_dir(self.home_dir.join(dir))
            .await
            .context("failed reading dir")?;
        let mut versions = vec![];
        while let Some(entry) = dir_entries.next_entry().await? {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            if file_type.is_dir() {
                if let Ok(name) = entry.file_name().into_string() {
                    versions.push(name);
                }
            }
        }
        Ok(versions)
    }
}

#[async_trait]
impl CompilerResolver for EnvCompilerResolver {
    async fn supported_versions(&self) -> anyhow::Result<SupportedCompilerVersions> {
        Ok(SupportedCompilerVersions {
            solc: self
                .read_dir("etc/solc-bin")
                .await
                .context("failed reading solc dir")?,
            zksolc: self
                .read_dir("etc/zksolc-bin")
                .await
                .context("failed reading zksolc dir")?,
            vyper: self
                .read_dir("etc/vyper-bin")
                .await
                .context("failed reading vyper dir")?,
            zkvyper: self
                .read_dir("etc/zkvyper-bin")
                .await
                .context("failed reading zkvyper dir")?,
        })
    }

    async fn resolve_solc(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<SolcInput>>, ContractVerifierError> {
        let solc_path = CompilerType::Solc.bin_path(&self.home_dir, version).await?;
        Ok(Box::new(Solc::new(solc_path)))
    }

    async fn resolve_zksolc(
        &self,
        versions: &CompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkSolcInput>>, ContractVerifierError> {
        let zksolc_version = versions.zk_compiler_version();
        let zksolc_path = CompilerType::ZkSolc
            .bin_path(&self.home_dir, zksolc_version)
            .await?;
        let solc_path = CompilerType::Solc
            .bin_path(&self.home_dir, versions.compiler_version())
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

    async fn resolve_zkvyper(
        &self,
        versions: &CompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkVyperInput>>, ContractVerifierError> {
        let zkvyper_path = CompilerType::ZkVyper
            .bin_path(&self.home_dir, versions.zk_compiler_version())
            .await?;
        let vyper_path = CompilerType::Vyper
            .bin_path(&self.home_dir, versions.compiler_version())
            .await?;
        let compiler_paths = CompilerPaths {
            base: vyper_path,
            zk: zkvyper_path,
        };
        Ok(Box::new(ZkVyper::new(compiler_paths)))
    }
}
