use std::{
    collections::HashSet,
    fmt,
    path::{Path, PathBuf},
};

use tokio::fs;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification::api::CompilationArtifacts;

pub(crate) use self::env::EnvCompilerResolver;
use crate::{
    compilers::{SolcInput, VyperInput, ZkSolcInput},
    error::ContractVerifierError,
    ZkCompilerVersions,
};

mod env;

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

    fn is_safe_version(version: &str) -> bool {
        !version.is_empty()
            && version.len() <= 64
            && version.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-')
            })
    }

    async fn bin_path(
        self,
        home_dir: &Path,
        version: &str,
    ) -> Result<PathBuf, ContractVerifierError> {
        if !Self::is_safe_version(version) {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                self.as_str(),
                version.to_owned(),
            ));
        }
        let path = self.bin_path_unchecked(home_dir, version);
        let compiler_root = path
            .parent()
            .and_then(Path::parent)
            .expect("compiler path must have a version and root directory");
        let (canonical_root, canonical_path) = match (
            fs::canonicalize(compiler_root).await,
            fs::canonicalize(&path).await,
        ) {
            (Ok(root), Ok(path)) => (root, path),
            _ => {
                return Err(ContractVerifierError::UnknownCompilerVersion(
                    self.as_str(),
                    version.to_owned(),
                ));
            }
        };
        let is_regular_file = fs::metadata(&canonical_path)
            .await
            .map(|metadata| metadata.is_file())
            .unwrap_or(false);
        if !canonical_path.starts_with(&canonical_root) || !is_regular_file {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                self.as_str(),
                version.to_owned(),
            ));
        }
        Ok(canonical_path)
    }
}

#[cfg(test)]
mod tests {
    use super::CompilerType;

    #[test]
    fn compiler_versions_are_not_paths() {
        for version in [
            "../../tmp/evil",
            "/tmp/evil",
            "file://tmp/evil",
            "v1.5.17/../../evil",
            "v1.5.17\\..\\evil",
            "",
        ] {
            assert!(!CompilerType::is_safe_version(version), "{version}");
        }
        for version in ["0.8.35", "v1.5.17", "zkVM-0.8.30-1.0.2"] {
            assert!(CompilerType::is_safe_version(version), "{version}");
        }
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
    pub fn lacks_any_compiler(&self) -> bool {
        self.solc.is_empty() || self.zksolc.is_empty()
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

    /// Resolves a `vyper` compiler. This capability is not exposed by the public policy.
    async fn resolve_vyper(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError>;

    /// Resolves a `zkvyper` compiler. This capability is not exposed by the public policy.
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
