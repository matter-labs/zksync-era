use std::{fmt, path::PathBuf};

use anyhow::Context as _;
use tokio::fs;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification_api::CompilerVersions;
use zksync_utils::env::Workspace;

use crate::error::ContractVerifierError;

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
    /// Resolves paths to `solc` / `zksolc`.
    async fn resolve_solc(
        &self,
        versions: &CompilerVersions,
    ) -> Result<CompilerPaths, ContractVerifierError>;

    /// Resolves paths to `vyper` / `zkvyper`.
    async fn resolve_vyper(
        &self,
        versions: &CompilerVersions,
    ) -> Result<CompilerPaths, ContractVerifierError>;
}

/// Default [`CompilerResolver`] using pre-downloaded compilers.
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

#[async_trait]
impl CompilerResolver for EnvCompilerResolver {
    async fn resolve_solc(
        &self,
        versions: &CompilerVersions,
    ) -> Result<CompilerPaths, ContractVerifierError> {
        let zksolc_version = versions.zk_compiler_version();
        let zksolc_path = self
            .home_dir
            .join("etc")
            .join("zksolc-bin")
            .join(&zksolc_version)
            .join("zksolc");
        if !fs::try_exists(&zksolc_path)
            .await
            .context("failed accessing zksolc")?
        {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "zksolc".to_owned(),
                zksolc_version,
            ));
        }

        let solc_version = versions.compiler_version();
        let solc_path = self
            .home_dir
            .join("etc")
            .join("solc-bin")
            .join(&solc_version)
            .join("solc");
        if !fs::try_exists(&solc_path)
            .await
            .context("failed accessing solc")?
        {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "solc".to_owned(),
                solc_version,
            ));
        }
        Ok(CompilerPaths {
            base: solc_path,
            zk: zksolc_path,
        })
    }

    async fn resolve_vyper(
        &self,
        versions: &CompilerVersions,
    ) -> Result<CompilerPaths, ContractVerifierError> {
        let zkvyper_version = versions.zk_compiler_version();
        let zkvyper_path = self
            .home_dir
            .join("etc")
            .join("zkvyper-bin")
            .join(&zkvyper_version)
            .join("zkvyper");
        if !fs::try_exists(&zkvyper_path)
            .await
            .context("failed accessing zkvyper")?
        {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "zkvyper".to_owned(),
                zkvyper_version,
            ));
        }

        let vyper_version = versions.compiler_version();
        let vyper_path = self
            .home_dir
            .join("etc")
            .join("vyper-bin")
            .join(&vyper_version)
            .join("vyper");
        if !fs::try_exists(&vyper_path)
            .await
            .context("failed accessing vyper")?
        {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "vyper".to_owned(),
                vyper_version,
            ));
        }
        Ok(CompilerPaths {
            base: vyper_path,
            zk: zkvyper_path,
        })
    }
}
