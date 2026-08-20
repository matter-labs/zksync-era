use std::{ffi::OsString, path, path::Path};

use anyhow::Context as _;
use tokio::{fs, io::AsyncWriteExt};
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification::api::CompilationArtifacts;

use super::{sanitize_compiler_stderr, VyperInput};
use crate::{
    error::ContractVerifierError,
    process::run_compiler,
    resolver::{Compiler, CompilerPaths},
};

impl VyperInput {
    async fn write_files(&self, root_dir: &Path) -> anyhow::Result<Vec<OsString>> {
        let mut paths = Vec::with_capacity(self.sources.len());
        for (name, content) in &self.sources {
            let mut name = name.clone();
            if !name.ends_with(".vy") {
                name += ".vy";
            }

            let name_path = Path::new(&name);
            anyhow::ensure!(
                !name_path.is_absolute(),
                "absolute contract filename: {name}"
            );
            let normal_components = name_path
                .components()
                .all(|component| matches!(component, path::Component::Normal(_)));
            anyhow::ensure!(
                normal_components,
                "contract filename contains disallowed components: {name}"
            );

            let path = root_dir.join(name_path);
            if let Some(prefix) = path.parent() {
                fs::create_dir_all(prefix)
                    .await
                    .with_context(|| format!("failed creating parent dir for `{name}`"))?;
            }
            let mut file = fs::File::create(&path)
                .await
                .with_context(|| format!("failed creating file for `{name}`"))?;
            file.write_all(content.as_bytes())
                .await
                .with_context(|| format!("failed writing to `{name}`"))?;
            paths.push(path.into_os_string());
        }
        Ok(paths)
    }
}

#[derive(Debug)]
pub(crate) struct ZkVyper {
    paths: CompilerPaths,
}

impl ZkVyper {
    pub fn new(paths: CompilerPaths) -> Self {
        Self { paths }
    }

    fn parse_output(
        output: &serde_json::Value,
        contract_name: String,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let file_name = format!("{contract_name}.vy");
        let object = output
            .as_object()
            .context("Vyper output is not an object")?;
        for (path, artifact) in object {
            let path = Path::new(&path);
            if path.file_name().unwrap().to_str().unwrap() == file_name {
                let bytecode_str = artifact["bytecode"]
                    .as_str()
                    .context("bytecode is not a string")?;
                let bytecode_without_prefix =
                    bytecode_str.strip_prefix("0x").unwrap_or(bytecode_str);
                let bytecode =
                    hex::decode(bytecode_without_prefix).context("failed decoding bytecode")?;
                return Ok(CompilationArtifacts {
                    abi: artifact["abi"].clone(),
                    bytecode,
                    deployed_bytecode: None,
                    immutable_refs: Default::default(),
                    factory_dependency_hashes: Default::default(),
                });
            }
        }
        Err(ContractVerifierError::MissingContract(contract_name))
    }
}

#[async_trait]
impl Compiler<VyperInput> for ZkVyper {
    async fn compile(
        self: Box<Self>,
        input: VyperInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let zkvyper_path = fs::canonicalize(&self.paths.zk)
            .await
            .context("failed to canonicalize zkvyper path")?;
        let vyper_path = fs::canonicalize(&self.paths.base)
            .await
            .context("failed to canonicalize vyper path")?;
        let mut command = tokio::process::Command::new(zkvyper_path);
        if let Some(o) = input.optimizer_mode.as_ref() {
            command.arg("-O").arg(o);
        }
        command
            .arg("--vyper")
            .arg(vyper_path)
            .arg("-f")
            .arg("combined_json");

        let temp_dir = tokio::task::spawn_blocking(tempfile::tempdir)
            .await
            .context("panicked creating temporary dir")?
            .context("failed creating temporary dir")?;
        let file_paths = input
            .write_files(temp_dir.path())
            .await
            .context("failed writing Vyper files to temp dir")?;
        command.current_dir(temp_dir.path()).args(file_paths);

        let output = run_compiler(&mut command, None).await?;
        if output.status.success() {
            let output = serde_json::from_slice(&output.stdout)
                .context("zkvyper output is not valid JSON")?;
            Self::parse_output(&output, input.contract_name)
        } else {
            Err(ContractVerifierError::CompilerError(
                "zkvyper",
                sanitize_compiler_stderr(&String::from_utf8_lossy(&output.stderr)),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[tokio::test]
    async fn sanitizing_contract_paths() {
        let mut input = VyperInput {
            contract_name: "Test".to_owned(),
            file_name: "test.vy".to_owned(),
            sources: HashMap::from([("/etc/shadow".to_owned(), String::new())]),
            optimizer_mode: None,
        };

        let temp_dir = tempfile::TempDir::new().unwrap();
        let err = input
            .write_files(temp_dir.path())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("absolute"), "{err}");

        input.sources = HashMap::from([("../../../etc/shadow".to_owned(), String::new())]);
        let err = input
            .write_files(temp_dir.path())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("disallowed components"), "{err}");
    }
}
