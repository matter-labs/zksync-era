use std::{collections::HashSet, path::PathBuf};

use anyhow::Context as _;
use tokio::fs;
use zksync_queued_job_processor::async_trait;
use zksync_utils::env::Workspace;

use crate::{
    compilers::{Solc, SolcInput, Vyper, VyperInput, ZkSolc, ZkSolcInput, ZkVyper},
    error::ContractVerifierError,
    resolver::{
        Compiler, CompilerPaths, CompilerResolver, CompilerType, SupportedCompilerVersions,
    },
    ZkCompilerVersions,
};

/// Default [`CompilerResolver`] using pre-downloaded compilers in the `/etc` subdirectories (relative to the workspace).
#[derive(Debug)]
pub(crate) struct EnvCompilerResolver {
    home_dir: PathBuf,
}

impl Default for EnvCompilerResolver {
    fn default() -> Self {
        Self {
            home_dir: Workspace::locate().root(),
        }
    }
}

impl EnvCompilerResolver {
    async fn read_dir(&self, dir: &str) -> anyhow::Result<HashSet<String>> {
        let mut dir_entries = fs::read_dir(self.home_dir.join(dir))
            .await
            .context("failed reading dir")?;
        let mut versions = HashSet::new();
        while let Some(entry) = dir_entries.next_entry().await? {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            if file_type.is_dir() {
                if let Ok(name) = entry.file_name().into_string() {
                    versions.insert(name);
                }
            }
        }
        Ok(versions)
    }
}

#[async_trait]
impl CompilerResolver for EnvCompilerResolver {
    async fn supported_versions(&self) -> anyhow::Result<SupportedCompilerVersions> {
        let versions = SupportedCompilerVersions {
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
        };
        tracing::info!("EnvResolver supported versions: {:?}", versions);

        Ok(versions)
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
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkSolcInput>>, ContractVerifierError> {
        let zksolc_version = &version.zk;
        let zksolc_path = CompilerType::ZkSolc
            .bin_path(&self.home_dir, zksolc_version)
            .await?;
        let solc_path = CompilerType::Solc
            .bin_path(&self.home_dir, &version.base)
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
        let vyper_path = CompilerType::Vyper
            .bin_path(&self.home_dir, version)
            .await?;
        Ok(Box::new(Vyper::new(vyper_path)))
    }

    async fn resolve_zkvyper(
        &self,
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError> {
        let zkvyper_path = CompilerType::ZkVyper
            .bin_path(&self.home_dir, &version.zk)
            .await?;
        let vyper_path = CompilerType::Vyper
            .bin_path(&self.home_dir, &version.base)
            .await?;
        let compiler_paths = CompilerPaths {
            base: vyper_path,
            zk: zkvyper_path,
        };
        Ok(Box::new(ZkVyper::new(compiler_paths)))
    }
}
