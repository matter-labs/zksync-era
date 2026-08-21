use std::{collections::HashMap, io::Write as _};

use anyhow::Context as _;
use regex::Regex;
use semver::Version;
use zksync_queued_job_processor::async_trait;
use zksync_types::contract_verification::api::{
    CompilationArtifacts, SourceCodeData, VerificationIncomingRequest,
};

use crate::{
    compilers::{
        parse_standard_json_input, parse_standard_json_output, process_contract_name,
        sanitize_compiler_stderr, validate_contract_target, CompilerFlavor, Optimizer, Settings,
        Source, StandardJson,
    },
    error::ContractVerifierError,
    process::run_compiler,
    resolver::{Compiler, CompilerPaths},
};

#[derive(Debug)]
pub(crate) enum ZkSolcInput {
    StandardJson {
        input: Box<StandardJson>,
        contract_name: String,
        file_name: String,
    },
    YulSingleFile {
        source_code: String,
        is_system: bool,
    },
}

#[derive(Debug)]
pub(crate) struct ZkSolc {
    paths: CompilerPaths,
    zksolc_version: String,
}

impl ZkSolc {
    pub fn new(paths: CompilerPaths, zksolc_version: String) -> Self {
        Self {
            paths,
            zksolc_version,
        }
    }

    pub fn build_input(
        req: VerificationIncomingRequest,
        zksolc_version: &str,
    ) -> Result<ZkSolcInput, ContractVerifierError> {
        let (file_name, contract_name) = process_contract_name(&req.contract_name, "sol");
        validate_contract_target(&file_name, &contract_name)?;
        let is_system = req.is_system;
        let force_evmla = req.force_evmla;

        match req.source_code_data {
            SourceCodeData::SolSingleFile(source_code) => {
                let source = Source {
                    content: source_code,
                };
                let sources = HashMap::from([(file_name.clone(), source)]);
                let optimizer_runs = req
                    .evm_specific
                    .optimizer_runs
                    .map(u32::try_from)
                    .transpose()
                    .map_err(|_| {
                        ContractVerifierError::UnsupportedVerificationInput(
                            "optimizer runs exceeds the allowed limit".to_owned(),
                        )
                    })?;
                let settings = Settings {
                    output_selection: Some(Self::required_output_selection(
                        &file_name,
                        &contract_name,
                        Self::is_post_1_5_0(zksolc_version),
                    )),
                    optimizer: Some(Optimizer {
                        enabled: Some(req.optimization_used),
                        runs: optimizer_runs,
                        mode: req.optimizer_mode,
                        ..Optimizer::default()
                    }),
                    evm_version: req.evm_specific.evm_version,
                    enable_eravm_extensions: is_system.then_some(true),
                    force_evmla: force_evmla.then_some(true),
                    ..Settings::default()
                };

                let input = StandardJson {
                    language: "Solidity".to_string(),
                    sources,
                    settings,
                };
                input.validate(CompilerFlavor::ZkSolc)?;
                Ok(ZkSolcInput::StandardJson {
                    input: Box::new(input),
                    contract_name,
                    file_name,
                })
            }
            SourceCodeData::StandardJsonInput(map) => {
                let mut compiler_input = parse_standard_json_input(map, CompilerFlavor::ZkSolc)?;
                if is_system {
                    compiler_input.settings.enable_eravm_extensions = Some(true);
                }
                if force_evmla {
                    compiler_input.settings.force_evmla = Some(true);
                }
                compiler_input.settings.output_selection = Some(Self::required_output_selection(
                    &file_name,
                    &contract_name,
                    Self::is_post_1_5_0(zksolc_version),
                ));
                Ok(ZkSolcInput::StandardJson {
                    input: Box::new(compiler_input),
                    contract_name,
                    file_name,
                })
            }
            SourceCodeData::YulSingleFile(source_code) => {
                let validation_input = StandardJson {
                    language: "Yul".to_owned(),
                    sources: HashMap::from([(
                        file_name,
                        Source {
                            content: source_code.clone(),
                        },
                    )]),
                    settings: Settings::default(),
                };
                validation_input.validate_yul(CompilerFlavor::ZkSolc)?;
                Ok(ZkSolcInput::YulSingleFile {
                    source_code,
                    is_system,
                })
            }
            SourceCodeData::VyperMultiFile(_) => {
                Err(ContractVerifierError::UnsupportedVerificationInput(
                    "Vyper verification is disabled".to_owned(),
                ))
            }
        }
    }

    fn parse_single_file_yul_output(
        output: &str,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let captures = if output.contains("Binary:\n") {
            Regex::new(r"Binary:\n([\da-f]+)")
                .unwrap()
                .captures(output)
                .with_context(|| format!("Yul output doesn't match regex. Output: {output}"))?
        } else {
            Regex::new(r"Contract `.*` bytecode: 0x([\da-f]+)")
                .unwrap()
                .captures(output)
                .with_context(|| format!("Yul output doesn't match regex. Output: {output}"))?
        };
        let bytecode = captures
            .get(1)
            .context("no matches in Yul output")?
            .as_str();
        let bytecode = hex::decode(bytecode).context("invalid Yul output bytecode")?;

        Ok(CompilationArtifacts {
            bytecode,
            deployed_bytecode: None,
            abi: serde_json::Value::Array(Vec::new()),
            immutable_refs: Default::default(),
            factory_dependency_hashes: Default::default(),
        })
    }

    fn required_output_selection(
        file_name: &str,
        contract_name: &str,
        is_post_1_5_0: bool,
    ) -> serde_json::Value {
        let mut output_selection = serde_json::json!({});
        let contract_outputs = if is_post_1_5_0 {
            &["abi", "evm"][..]
        } else {
            &["abi"][..]
        };

        Self::ensure_selector_outputs(&mut output_selection, "*", "*", &["abi"]);
        Self::ensure_selector_outputs(&mut output_selection, "*", "", &["abi"]);
        Self::ensure_selector_outputs(
            &mut output_selection,
            file_name,
            contract_name,
            contract_outputs,
        );
        output_selection
    }

    fn ensure_selector_outputs(
        output_selection: &mut serde_json::Value,
        file_name: &str,
        contract_name: &str,
        outputs: &[&str],
    ) {
        if !output_selection.is_object() {
            *output_selection = serde_json::json!({});
        }
        let output_selection = output_selection.as_object_mut().unwrap();
        let file_selection = output_selection
            .entry(file_name.to_owned())
            .or_insert_with(|| serde_json::json!({}));
        if !file_selection.is_object() {
            *file_selection = serde_json::json!({});
        }
        let file_selection = file_selection.as_object_mut().unwrap();
        let contract_selection = file_selection
            .entry(contract_name.to_owned())
            .or_insert_with(|| serde_json::json!([]));
        if !contract_selection.is_array() {
            *contract_selection = serde_json::json!([]);
        }
        let contract_selection = contract_selection.as_array_mut().unwrap();

        for output in outputs {
            if !contract_selection
                .iter()
                .any(|selected| selected.as_str() == Some(output))
            {
                contract_selection.push(serde_json::Value::String((*output).to_owned()));
            }
        }
    }

    fn is_post_1_5_0(zksolc_version: &str) -> bool {
        // Special case
        if zksolc_version == "vm-1.5.0-a167aa3" {
            false
        } else {
            let version = zksolc_version.strip_prefix('v').unwrap_or(zksolc_version);
            if let Ok(semver) = Version::parse(version) {
                let target = Version::new(1, 5, 0);
                semver >= target
            } else {
                true
            }
        }
    }
}

#[async_trait]
impl Compiler<ZkSolcInput> for ZkSolc {
    async fn compile(
        self: Box<Self>,
        input: ZkSolcInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        // Resolve both binaries to absolute paths so they stay locatable after `current_dir` is
        // switched to the empty working directory in the standard-JSON branch below.
        let zksolc_path = tokio::fs::canonicalize(&self.paths.zk)
            .await
            .context("failed to canonicalize zksolc path")?;
        let solc_path = tokio::fs::canonicalize(&self.paths.base)
            .await
            .context("failed to canonicalize solc path")?;

        let mut command = tokio::process::Command::new(&zksolc_path);
        match &input {
            ZkSolcInput::StandardJson { input, .. } => {
                if !Self::is_post_1_5_0(&self.zksolc_version) {
                    if input.settings.system_mode_enabled() {
                        command.arg("--system-mode");
                    }
                    if input.settings.force_evmla_enabled() {
                        command.arg("--force-evmla");
                    }
                }
                command.arg("--solc").arg(&solc_path);
            }
            ZkSolcInput::YulSingleFile { is_system, .. } => {
                if Self::is_post_1_5_0(&self.zksolc_version) {
                    if *is_system {
                        command.arg("--enable-eravm-extensions");
                    } else {
                        command.arg("--solc").arg(&solc_path);
                    }
                } else {
                    if *is_system {
                        command.arg("--system-mode");
                    }
                    command.arg("--solc").arg(&solc_path);
                }
            }
        }

        match input {
            ZkSolcInput::StandardJson {
                input,
                contract_name,
                file_name,
            } => {
                // Run solc (invoked internally by zksolc) from an empty temp dir so standard-JSON
                // imports must be provided by the input source map.
                let compile_dir =
                    tempfile::tempdir().context("failed to create temp dir for zksolc")?;
                let content = serde_json::to_vec(&input)
                    .context("cannot encode standard JSON input for zksolc")?;
                command
                    .current_dir(compile_dir.path())
                    .arg("--standard-json")
                    .arg("--allow-paths")
                    .arg(compile_dir.path());

                let output = run_compiler(&mut command, Some(&content)).await?;
                if output.status.success() {
                    let output = serde_json::from_slice(&output.stdout)
                        .context("zksolc output is not valid JSON")?;
                    parse_standard_json_output(&output, contract_name, file_name, false)
                } else {
                    Err(ContractVerifierError::CompilerError(
                        "zksolc",
                        sanitize_compiler_stderr(&String::from_utf8_lossy(&output.stderr)),
                    ))
                }
            }
            ZkSolcInput::YulSingleFile { source_code, .. } => {
                let compile_dir =
                    tempfile::tempdir().context("cannot create temporary Yul directory")?;
                let source_path = compile_dir.path().join("input.yul");
                let mut source_file = std::fs::File::create(&source_path)
                    .context("cannot create temporary Yul file")?;
                source_file
                    .write_all(source_code.as_bytes())
                    .context("failed writing Yul file")?;
                drop(source_file);

                command
                    .current_dir(compile_dir.path())
                    .arg(&source_path)
                    .arg("--optimization")
                    .arg("3")
                    .arg("--yul")
                    .arg("--bin");
                let output = run_compiler(&mut command, None).await?;
                if output.status.success() {
                    let output =
                        String::from_utf8(output.stdout).context("zksolc output is not UTF-8")?;
                    Self::parse_single_file_yul_output(&output)
                } else {
                    Err(ContractVerifierError::CompilerError(
                        "zksolc",
                        sanitize_compiler_stderr(&String::from_utf8_lossy(&output.stderr)),
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::contract_verification::api::{
        CompilerVersions, SourceCodeData, VerificationIncomingRequest,
    };

    use super::*;

    const COUNTER_CONTRACT: &str = r#"
        contract Counter {
            function value() external pure returns (uint256) {
                return 42;
            }
        }
    "#;

    fn standard_json_request(output_selection: serde_json::Value) -> VerificationIncomingRequest {
        VerificationIncomingRequest {
            contract_address: Default::default(),
            source_code_data: SourceCodeData::StandardJsonInput(
                serde_json::json!({
                    "language": "Solidity",
                    "sources": {
                        "contracts/Counter.sol": {
                            "content": COUNTER_CONTRACT,
                        },
                    },
                    "settings": {
                        "outputSelection": output_selection,
                        "optimizer": {
                            "enabled": true,
                        }
                    },
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            contract_name: "contracts/Counter.sol:Counter".to_owned(),
            compiler_versions: CompilerVersions::Solc {
                compiler_solc_version: "zkVM-0.8.26-1.0.2".to_owned(),
                compiler_zksolc_version: Some("v1.5.0".to_owned()),
            },
            optimization_used: true,
            optimizer_mode: None,
            constructor_arguments: Default::default(),
            is_system: false,
            force_evmla: false,
            evm_specific: Default::default(),
        }
    }

    fn standard_json_input(input: &ZkSolcInput) -> &StandardJson {
        let ZkSolcInput::StandardJson { input, .. } = input else {
            panic!("expected standard JSON input: {input:?}");
        };
        input
    }

    fn assert_selector_contains(
        output_selection: &serde_json::Value,
        file_name: &str,
        contract_name: &str,
        expected_outputs: &[&str],
    ) {
        let selected_outputs = output_selection
            .get(file_name)
            .and_then(serde_json::Value::as_object)
            .and_then(|file_selection| file_selection.get(contract_name))
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| {
                panic!("missing selector {file_name:?} / {contract_name:?}: {output_selection}")
            });

        for expected_output in expected_outputs {
            assert!(
                selected_outputs
                    .iter()
                    .any(|output| output.as_str() == Some(expected_output)),
                "selector {file_name:?} / {contract_name:?} is missing {expected_output:?}: {selected_outputs:?}"
            );
        }
    }

    fn assert_selector_excludes(
        output_selection: &serde_json::Value,
        file_name: &str,
        contract_name: &str,
        excluded_output: &str,
    ) {
        let selected_outputs = output_selection
            .get(file_name)
            .and_then(serde_json::Value::as_object)
            .and_then(|file_selection| file_selection.get(contract_name))
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| {
                panic!("missing selector {file_name:?} / {contract_name:?}: {output_selection}")
            });

        assert!(
            !selected_outputs
                .iter()
                .any(|output| output.as_str() == Some(excluded_output)),
            "selector {file_name:?} / {contract_name:?} must not include {excluded_output:?}: {selected_outputs:?}"
        );
    }

    #[test]
    fn legacy_zksolc_output_selection_does_not_add_evm_selector() {
        let req = standard_json_request(serde_json::json!({
            "*": {
                "*": ["metadata", "evm.methodIdentifiers"],
                "": ["ast"],
            }
        }));

        let input = ZkSolc::build_input(req, "v1.3.13").unwrap();
        let standard_json = standard_json_input(&input);
        let output_selection = standard_json.settings.output_selection.as_ref().unwrap();

        assert_selector_contains(output_selection, "*", "*", &["abi"]);
        assert_selector_excludes(output_selection, "*", "*", "evm");
        assert_selector_contains(output_selection, "*", "", &["abi"]);
        assert_selector_contains(
            output_selection,
            "contracts/Counter.sol",
            "Counter",
            &["abi"],
        );
        assert_selector_excludes(output_selection, "contracts/Counter.sol", "Counter", "evm");
    }

    #[test]
    fn post_1_5_0_zksolc_output_selection_adds_evm_selector() {
        let req = standard_json_request(serde_json::json!({
            "contracts/Counter.sol": {
                "Counter": ["metadata"],
            }
        }));

        let input = ZkSolc::build_input(req, "v1.5.0").unwrap();
        let standard_json = standard_json_input(&input);
        let output_selection = standard_json.settings.output_selection.as_ref().unwrap();

        assert_selector_contains(output_selection, "*", "*", &["abi"]);
        assert_selector_excludes(output_selection, "*", "*", "evm");
        assert_selector_contains(output_selection, "*", "", &["abi"]);
        assert_selector_excludes(output_selection, "*", "", "evm");
        assert_selector_contains(
            output_selection,
            "contracts/Counter.sol",
            "Counter",
            &["abi", "evm"],
        );
    }

    #[test]
    fn check_is_post_1_5_0() {
        assert!(
            !ZkSolc::is_post_1_5_0("vm-1.5.0-a167aa3"),
            "vm-1.5.0-a167aa3"
        );
        assert!(ZkSolc::is_post_1_5_0("v1.5.0"), "v1.5.0");
        assert!(ZkSolc::is_post_1_5_0("v1.5.1"), "v1.5.1");
        assert!(ZkSolc::is_post_1_5_0("v1.10.1"), "v1.10.1");
        assert!(ZkSolc::is_post_1_5_0("v2.0.0"), "v2.0.0");
        assert!(!ZkSolc::is_post_1_5_0("v1.4.15"), "v1.4.15");
        assert!(!ZkSolc::is_post_1_5_0("v1.3.21"), "v1.3.21");
        assert!(!ZkSolc::is_post_1_5_0("v0.5.1"), "v0.5.1");
    }

    #[test]
    fn build_input_replaces_existing_standard_json_output_selection() {
        let req = VerificationIncomingRequest {
            contract_address: Default::default(),
            source_code_data: SourceCodeData::StandardJsonInput(
                serde_json::json!({
                    "language": "Solidity",
                    "sources": {
                        "Counter.sol": {
                            "content": "contract Counter { function value() external pure returns (uint256) { return 1; } }",
                        }
                    },
                    "settings": {
                        "outputSelection": {
                            "*": {
                                "*": ["storageLayout"],
                                "": ["ast"]
                            },
                            "Counter.sol": {
                                "Counter": ["abi"]
                            }
                        }
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            contract_name: "Counter".to_owned(),
            compiler_versions: CompilerVersions::Solc {
                compiler_solc_version: "0.8.27".to_owned(),
                compiler_zksolc_version: Some("1.5.4".to_owned()),
            },
            optimization_used: true,
            optimizer_mode: None,
            constructor_arguments: Default::default(),
            is_system: false,
            force_evmla: false,
            evm_specific: Default::default(),
        };

        let input = ZkSolc::build_input(req, "1.5.4").unwrap();
        let input = standard_json_input(&input);

        assert_eq!(
            input.settings.output_selection,
            Some(serde_json::json!({
                "*": {
                    "*": ["abi"],
                    "": ["abi"]
                },
                "Counter.sol": {
                    "Counter": ["abi", "evm"]
                }
            }))
        );
    }

    #[test]
    fn build_input_rejects_root_level_standard_json_fields() {
        let req = VerificationIncomingRequest {
            contract_address: Default::default(),
            source_code_data: SourceCodeData::StandardJsonInput(
                serde_json::json!({
                    "language": "Solidity",
                    "sources": {
                        "Counter.sol": {
                            "content": "contract Counter { function value() external pure returns (uint256) { return 1; } }",
                        }
                    },
                    "suppressedErrors": ["sendtransfer"],
                    "suppressedWarnings": ["txorigin"],
                    "settings": {
                        "outputSelection": {
                            "*": {
                                "*": ["abi"]
                            }
                        }
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            contract_name: "Counter".to_owned(),
            compiler_versions: CompilerVersions::Solc {
                compiler_solc_version: "0.8.27".to_owned(),
                compiler_zksolc_version: Some("1.5.4".to_owned()),
            },
            optimization_used: true,
            optimizer_mode: None,
            constructor_arguments: Default::default(),
            is_system: false,
            force_evmla: false,
            evm_specific: Default::default(),
        };

        assert!(matches!(
            ZkSolc::build_input(req, "1.5.4"),
            Err(ContractVerifierError::FailedToDeserializeInput)
        ));
    }

    #[test]
    fn build_input_rejects_llvm_options() {
        let mut req = standard_json_request(serde_json::json!({}));
        let SourceCodeData::StandardJsonInput(input) = &mut req.source_code_data else {
            unreachable!();
        };
        input["settings"]["LLVMOptions"] = serde_json::json!(["--exec-on-ir-change=/bin/sh"]);

        assert!(matches!(
            ZkSolc::build_input(req, "1.5.17"),
            Err(ContractVerifierError::FailedToDeserializeInput)
        ));
    }

    #[test]
    fn build_input_retains_private_request_modes() {
        let mut system_req = standard_json_request(serde_json::json!({}));
        system_req.is_system = true;
        let system_input = ZkSolc::build_input(system_req, "v1.5.17").unwrap();
        assert!(standard_json_input(&system_input)
            .settings
            .system_mode_enabled());

        let mut evmla_req = standard_json_request(serde_json::json!({}));
        evmla_req.force_evmla = true;
        let evmla_input = ZkSolc::build_input(evmla_req, "v1.5.17").unwrap();
        assert!(standard_json_input(&evmla_input)
            .settings
            .force_evmla_enabled());
    }

    #[test]
    fn build_input_retains_private_yul_capability() {
        let mut req = standard_json_request(serde_json::json!({}));
        req.source_code_data = SourceCodeData::YulSingleFile("object \"Empty\" {}".to_owned());
        req.contract_name = "Empty".to_owned();
        req.is_system = true;

        assert!(matches!(
            ZkSolc::build_input(req, "v1.5.17"),
            Ok(ZkSolcInput::YulSingleFile {
                is_system: true,
                ..
            })
        ));
    }
}
