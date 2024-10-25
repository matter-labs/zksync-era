use std::{collections::HashMap, fs::File, io::Write, path::PathBuf, process::Stdio};

use anyhow::Context as _;

use crate::error::ContractVerifierError;

#[derive(Debug)]
pub struct ZkVyperInput {
    pub sources: HashMap<String, String>,
    pub optimizer_mode: Option<String>,
}

pub struct ZkVyper {
    zkvyper_path: PathBuf,
    vyper_path: PathBuf,
}

impl ZkVyper {
    pub fn new(zkvyper_path: impl Into<PathBuf>, vyper_path: impl Into<PathBuf>) -> Self {
        ZkVyper {
            zkvyper_path: zkvyper_path.into(),
            vyper_path: vyper_path.into(),
        }
    }

    pub async fn async_compile(
        &self,
        input: ZkVyperInput,
    ) -> Result<serde_json::Value, ContractVerifierError> {
        let mut command = tokio::process::Command::new(&self.zkvyper_path);
        if let Some(o) = input.optimizer_mode.as_ref() {
            command.arg("-O").arg(o);
        }
        command
            .arg("--vyper")
            .arg(self.vyper_path.to_str().unwrap())
            .arg("-f")
            .arg("combined_json")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let temp_dir = tempfile::tempdir().context("failed creating temporary dir")?;
        for (mut name, content) in input.sources {
            if !name.ends_with(".vy") {
                name += ".vy";
            }
            let path = temp_dir.path().join(&name);
            if let Some(prefix) = path.parent() {
                std::fs::create_dir_all(prefix)
                    .with_context(|| format!("failed creating parent dir for `{name}`"))?;
            }
            let mut file = File::create(&path)
                .with_context(|| format!("failed creating file for `{name}`"))?;
            file.write_all(content.as_bytes())
                .with_context(|| format!("failed writing to `{name}`"))?;
            command.arg(path.into_os_string());
        }

        let child = command.spawn().context("cannot spawn zkvyper")?;
        let output = child.wait_with_output().await.context("zkvyper failed")?;
        if output.status.success() {
            Ok(serde_json::from_slice(&output.stdout)
                .context("zkvyper output is not valid JSON")?)
        } else {
            Err(ContractVerifierError::CompilerError(
                "zkvyper".to_string(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }
}
