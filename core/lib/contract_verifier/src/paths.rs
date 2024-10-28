use std::path::{Path, PathBuf};

use anyhow::Context as _;
use tokio::fs;
use zksync_types::contract_verification_api::CompilerVersions;
use zksync_utils::env::Workspace;

use crate::error::ContractVerifierError;

fn home_path() -> PathBuf {
    Workspace::locate().core()
}

#[derive(Debug)]
pub(crate) struct CompilerPaths {
    /// Path to the base (non-zk) compiler.
    pub base: PathBuf,
    /// Path to the zk compiler.
    pub zk: PathBuf,
}

impl CompilerPaths {
    pub async fn for_solc(versions: &CompilerVersions) -> Result<Self, ContractVerifierError> {
        let zksolc_version = versions.zk_compiler_version();
        let zksolc_path = Path::new(&home_path())
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
        let solc_path = Path::new(&home_path())
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
        Ok(Self {
            base: solc_path,
            zk: zksolc_path,
        })
    }

    pub async fn for_vyper(versions: &CompilerVersions) -> Result<Self, ContractVerifierError> {
        let zkvyper_version = versions.zk_compiler_version();
        let zkvyper_path = Path::new(&home_path())
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
        let vyper_path = Path::new(&home_path())
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
        Ok(Self {
            base: vyper_path,
            zk: zkvyper_path,
        })
    }
}
