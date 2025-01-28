use std::{
    collections::HashSet,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context as _;
use tokio::fs;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification::api::CompilationArtifacts;

pub(crate) use self::{env::EnvCompilerResolver, github::GitHubCompilerResolver};
use crate::{
    compilers::{SolcInput, VyperInput, ZkSolcInput},
    error::ContractVerifierError,
    ZkCompilerVersions,
};

mod env;
mod github;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    async fn exists(self, home_dir: &Path, version: &str) -> Result<bool, ContractVerifierError> {
        let path = self.bin_path_unchecked(home_dir, version);
        let exists = fs::try_exists(&path)
            .await
            .with_context(|| format!("failed accessing `{}`", self.as_str()))?;
        Ok(exists)
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
#[derive(Debug, Default)]
pub(crate) struct SupportedCompilerVersions {
    /// Note: solc can have two "flavors": "upstream" solc (e.g. "real" solc used for L1 development),
    /// and "zksync" solc (e.g. ZKsync fork of the solc used by `zksolc`).
    /// They both are considered as "solc", but they have different versioning scheme, e.g.
    /// "upstream" solc can have version `0.8.0`, while "zksync" solc can have version `zkVM-0.8.0-1.0.1`.
    pub solc: HashSet<String>,
    pub zksolc: HashSet<String>,
    pub vyper: HashSet<String>,
    pub zkvyper: HashSet<String>,
}

impl SupportedCompilerVersions {
    fn merge(&mut self, other: SupportedCompilerVersions) {
        self.solc.extend(other.solc);
        self.zksolc.extend(other.zksolc);
        self.vyper.extend(other.vyper);
        self.zkvyper.extend(other.zkvyper);
    }
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
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkSolcInput>>, ContractVerifierError>;

    /// Resolves a `vyper` compiler.
    async fn resolve_vyper(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError>;

    /// Resolves a `zkvyper` compiler.
    async fn resolve_zkvyper(
        &self,
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError>;
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

#[derive(Debug)]
pub struct ResolverMultiplexer {
    resolvers: Vec<Arc<dyn CompilerResolver>>,
}

impl ResolverMultiplexer {
    pub fn new(resolver: Arc<dyn CompilerResolver>) -> Self {
        Self {
            resolvers: vec![resolver],
        }
    }

    pub fn with_resolver(mut self, resolver: Arc<dyn CompilerResolver>) -> Self {
        self.resolvers.push(resolver);
        self
    }
}

#[async_trait]
impl CompilerResolver for ResolverMultiplexer {
    async fn supported_versions(&self) -> anyhow::Result<SupportedCompilerVersions> {
        let mut versions = SupportedCompilerVersions::default();
        for resolver in &self.resolvers {
            versions.merge(resolver.supported_versions().await?);
        }
        Ok(versions)
    }

    /// Resolves a `solc` compiler.
    async fn resolve_solc(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<SolcInput>>, ContractVerifierError> {
        for resolver in &self.resolvers {
            match resolver.resolve_solc(version).await {
                Ok(compiler) => return Ok(compiler),
                Err(ContractVerifierError::UnknownCompilerVersion(..)) => {
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        Err(ContractVerifierError::UnknownCompilerVersion(
            "solc",
            version.to_owned(),
        ))
    }

    /// Resolves a `zksolc` compiler.
    async fn resolve_zksolc(
        &self,
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkSolcInput>>, ContractVerifierError> {
        let mut last_error = Err(ContractVerifierError::UnknownCompilerVersion(
            "zksolc",
            version.zk.to_owned(),
        ));
        for resolver in &self.resolvers {
            match resolver.resolve_zksolc(version).await {
                Ok(compiler) => return Ok(compiler),
                err @ Err(ContractVerifierError::UnknownCompilerVersion(..)) => {
                    last_error = err;
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        last_error
    }

    /// Resolves a `vyper` compiler.
    async fn resolve_vyper(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError> {
        for resolver in &self.resolvers {
            match resolver.resolve_vyper(version).await {
                Ok(compiler) => return Ok(compiler),
                Err(ContractVerifierError::UnknownCompilerVersion(..)) => {
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        Err(ContractVerifierError::UnknownCompilerVersion(
            "vyper",
            version.to_owned(),
        ))
    }

    /// Resolves a `zkvyper` compiler.
    async fn resolve_zkvyper(
        &self,
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError> {
        let mut last_error = Err(ContractVerifierError::UnknownCompilerVersion(
            "zkvyper",
            version.zk.to_owned(),
        ));
        for resolver in &self.resolvers {
            match resolver.resolve_zkvyper(version).await {
                Ok(compiler) => return Ok(compiler),
                err @ Err(ContractVerifierError::UnknownCompilerVersion(..)) => {
                    last_error = err;
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        last_error
    }
}
