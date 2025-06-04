use std::{collections::HashMap, fmt};

use chrono::{DateTime, Utc};
use serde::{
    de::{Deserializer, Error, MapAccess, Unexpected, Visitor},
    Deserialize, Serialize,
};
use zksync_basic_types::bytecode::BytecodeMarker;

use crate::{contract_verification::contract_identifier::CborMetadata, web3::Bytes, Address};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "codeFormat", content = "sourceCode")]
pub enum SourceCodeData {
    #[serde(rename = "solidity-single-file")]
    SolSingleFile(String),
    #[serde(rename = "solidity-standard-json-input")]
    StandardJsonInput(serde_json::Map<String, serde_json::Value>),
    #[serde(rename = "vyper-multi-file")]
    VyperMultiFile(HashMap<String, String>),
    #[serde(rename = "yul-single-file")]
    YulSingleFile(String),
}

impl SourceCodeData {
    pub fn compiler_type(&self) -> CompilerType {
        match self {
            SourceCodeData::SolSingleFile(_)
            | SourceCodeData::StandardJsonInput(_)
            | SourceCodeData::YulSingleFile(_) => CompilerType::Solc,
            SourceCodeData::VyperMultiFile(_) => CompilerType::Vyper,
        }
    }
}

// Implementing Custom deserializer which deserializes `SourceCodeData`
// as `SingleFile` if `codeFormat` is not specified.
// Serde doesn't support this feature: https://github.com/serde-rs/serde/issues/2231
impl<'de> Deserialize<'de> for SourceCodeData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(SourceCodeVisitor)
    }
}

struct SourceCodeVisitor;

impl<'de> Visitor<'de> for SourceCodeVisitor {
    type Value = SourceCodeData;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("source code data")
    }
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut source_code = None;
        let mut r#type = None;
        while let Some(key) = map.next_key::<String>()? {
            match &*key {
                "sourceCode" => source_code = Some(map.next_value::<serde_json::Value>()?),
                "codeFormat" => r#type = Some(map.next_value::<String>()?),
                _ => continue,
            }
        }
        let result = match r#type.as_deref() {
            Some("solidity-single-file") | None => {
                let value = source_code.ok_or_else(|| A::Error::missing_field("source_code"))?;
                SourceCodeData::SolSingleFile(
                    value
                        .as_str()
                        .ok_or_else(|| {
                            A::Error::invalid_type(Unexpected::Other(&value.to_string()), &self)
                        })?
                        .to_string(),
                )
            }
            Some("yul-single-file") => {
                let value = source_code.ok_or_else(|| A::Error::missing_field("source_code"))?;
                SourceCodeData::YulSingleFile(
                    value
                        .as_str()
                        .ok_or_else(|| {
                            A::Error::invalid_type(Unexpected::Other(&value.to_string()), &self)
                        })?
                        .to_string(),
                )
            }
            Some("solidity-standard-json-input") => {
                let value = source_code.ok_or_else(|| A::Error::missing_field("source_code"))?;
                SourceCodeData::StandardJsonInput(
                    value
                        .as_object()
                        .ok_or_else(|| {
                            A::Error::invalid_type(Unexpected::Other(&value.to_string()), &self)
                        })?
                        .clone(),
                )
            }
            Some("vyper-multi-file") => {
                let value = source_code.ok_or_else(|| A::Error::missing_field("source_code"))?;
                let obj = value
                    .as_object()
                    .ok_or_else(|| {
                        A::Error::invalid_type(Unexpected::Other(&value.to_string()), &self)
                    })?
                    .clone();
                let sources = serde_json::from_value(serde_json::Value::Object(obj))
                    .map_err(|_| A::Error::custom("invalid object"))?;
                SourceCodeData::VyperMultiFile(sources)
            }
            Some(x) => {
                return Err(A::Error::unknown_variant(
                    x,
                    &[
                        "solidity-single-file",
                        "solidity-standard-json-input",
                        "yul-single-file",
                        "vyper-multi-file",
                    ],
                ))
            }
        };
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationIncomingRequest {
    pub contract_address: Address,
    #[serde(flatten)]
    pub source_code_data: SourceCodeData,
    pub contract_name: String,
    #[serde(flatten)]
    pub compiler_versions: CompilerVersions,
    pub optimization_used: bool,
    /// Optimization mode used for the contract. Semantics depends on the compiler used; e.g., for `vyper`,
    /// allowed values are `gas` (default), `codesize` or `none`.
    pub optimizer_mode: Option<String>,
    #[serde(default)]
    pub constructor_arguments: Bytes,
    #[serde(default, alias = "enableEraVMExtensions")]
    pub is_system: bool,
    #[serde(default)]
    pub force_evmla: bool,
    #[serde(flatten)]
    pub evm_specific: VerificationEvmSettings,
}

impl VerificationIncomingRequest {
    fn get_metadata_versions(&self, cbor_metadata: &CborMetadata) -> (String, Option<String>) {
        let (compiler_version, zk_compiler_version) = cbor_metadata.get_compiler_versions();
        let request_compiler = self.compiler_versions.compiler_version();
        let request_zk_compiler = self.compiler_versions.zk_compiler_version();

        // If the metadata doesn't contain the compiler version, we assume that it is the same as in the request.
        let metadata_compiler = compiler_version.unwrap_or(request_compiler.to_string());
        let metadata_zk_compiler = zk_compiler_version.or(request_zk_compiler.map(str::to_string));

        (metadata_compiler, metadata_zk_compiler)
    }

    /// Checks if the compiler versions in the request and metadata match.
    pub fn compiler_versions_match(&self, cbor_metadata: &CborMetadata) -> bool {
        let (metadata_compiler, metadata_zk_compiler) = self.get_metadata_versions(cbor_metadata);

        self.compiler_versions.compiler_version() == metadata_compiler
            && self.compiler_versions.zk_compiler_version() == metadata_zk_compiler.as_deref()
    }

    /// Updates compiler versions for the request with the versions retrieved from the Cbor metadata.
    pub fn with_updated_compiler_versions(mut self, cbor_metadata: &CborMetadata) -> Self {
        let (metadata_compiler, metadata_zk_compiler) = self.get_metadata_versions(cbor_metadata);

        match self.compiler_versions.compiler_type() {
            CompilerType::Solc => {
                self.compiler_versions = CompilerVersions::Solc {
                    compiler_solc_version: metadata_compiler,
                    compiler_zksolc_version: metadata_zk_compiler,
                };
            }
            CompilerType::Vyper => {
                self.compiler_versions = CompilerVersions::Vyper {
                    compiler_vyper_version: metadata_compiler,
                    compiler_zkvyper_version: metadata_zk_compiler,
                };
            }
        }
        self
    }
}

/// Settings for EVM verification, used only if
/// `SourceCodeData` is `SolSingleFile`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationEvmSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evm_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimizer_runs: Option<u16>,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum CompilerType {
    Solc,
    Vyper,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompilerVersions {
    #[serde(rename_all = "camelCase")]
    Solc {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compiler_zksolc_version: Option<String>,
        compiler_solc_version: String,
    },
    #[serde(rename_all = "camelCase")]
    Vyper {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compiler_zkvyper_version: Option<String>,
        compiler_vyper_version: String,
    },
}

impl CompilerVersions {
    pub fn compiler_type(&self) -> CompilerType {
        match self {
            CompilerVersions::Solc { .. } => CompilerType::Solc,
            CompilerVersions::Vyper { .. } => CompilerType::Vyper,
        }
    }

    pub fn zk_compiler_version(&self) -> Option<&str> {
        match self {
            Self::Solc {
                compiler_zksolc_version,
                ..
            } => compiler_zksolc_version.as_deref(),
            Self::Vyper {
                compiler_zkvyper_version,
                ..
            } => compiler_zkvyper_version.as_deref(),
        }
    }

    pub fn compiler_version(&self) -> &str {
        match self {
            Self::Solc {
                compiler_solc_version,
                ..
            } => compiler_solc_version,
            Self::Vyper {
                compiler_vyper_version,
                ..
            } => compiler_vyper_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRequest {
    pub id: usize,
    #[serde(flatten)]
    pub req: VerificationIncomingRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilationArtifacts {
    /// In case of EVM contracts, this is the creation bytecode (`bytecode` in `solc` output).
    pub bytecode: Vec<u8>,
    /// Deployed bytecode (`deployedBytecode` in `solc` output). Only set for EVM contracts; for EraVM contracts, the deployed bytecode
    /// is always `bytecode` (i.e., there's no distinction between creation and deployed bytecodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed_bytecode: Option<Vec<u8>>,
    pub abi: serde_json::Value,
    /// Map of placeholders -> list of offsets for each immutable slot.
    /// Defaults to empty if no immutables are found.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub immutable_refs: HashMap<String, Vec<ImmutableReference>>,
}

/// Stores each immutable reference offset and length in deployed bytecode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmutableReference {
    pub start: usize,
    pub length: usize,
}

impl CompilationArtifacts {
    pub fn deployed_bytecode(&self) -> &[u8] {
        self.deployed_bytecode.as_deref().unwrap_or(&self.bytecode)
    }

    /// Patches the provided `compiled_code` and `deployed_code` slices by zeroing
    /// out the bytes corresponding to each immutable reference.
    pub fn patch_immutable_bytecodes(&self, compiled_code: &mut [u8], deployed_code: &mut [u8]) {
        for spans in self.immutable_refs.values() {
            for span in spans {
                let start = span.start;
                let end = start + span.length;
                if end <= compiled_code.len() && end <= deployed_code.len() {
                    compiled_code[start..end].fill(0);
                    deployed_code[start..end].fill(0);
                }
            }
        }
    }
}

/// Non-critical issues detected during verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VerificationProblem {
    /// The bytecode is correct, but metadata hash is different.
    IncorrectMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationInfo {
    pub request: VerificationRequest,
    pub artifacts: CompilationArtifacts,
    pub verified_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verification_problems: Vec<VerificationProblem>,
}

impl VerificationInfo {
    pub fn is_perfect_match(&self) -> bool {
        self.verification_problems.is_empty()
    }

    pub fn bytecode_marker(&self) -> BytecodeMarker {
        // Deployed bytecode is only present for EVM contracts.
        if self.artifacts.deployed_bytecode.is_some() {
            BytecodeMarker::Evm
        } else {
            BytecodeMarker::EraVm
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRequestStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compilation_errors: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;
    use crate::contract_verification::contract_identifier::{CborCompilerVersion, CborMetadata};

    #[test]
    fn source_code_deserialization() {
        let single_file_str = r#"{"codeFormat": "solidity-single-file", "sourceCode": "text"}"#;
        let single_file_result = serde_json::from_str::<SourceCodeData>(single_file_str);
        assert_matches!(single_file_result, Ok(SourceCodeData::SolSingleFile(_)));

        let stand_json_input_str =
            r#"{"codeFormat": "solidity-standard-json-input", "sourceCode": {}}"#;
        let stand_json_input_result = serde_json::from_str::<SourceCodeData>(stand_json_input_str);
        assert_matches!(
            stand_json_input_result,
            Ok(SourceCodeData::StandardJsonInput(_))
        );

        let type_not_specified_str = r#"{"sourceCode": "text"}"#;
        let type_not_specified_result =
            serde_json::from_str::<SourceCodeData>(type_not_specified_str);
        assert_matches!(
            type_not_specified_result,
            Ok(SourceCodeData::SolSingleFile(_))
        );

        let type_not_specified_object_str = r#"{"sourceCode": {}}"#;
        let type_not_specified_object_result =
            serde_json::from_str::<SourceCodeData>(type_not_specified_object_str);
        assert!(type_not_specified_object_result.is_err());
    }

    fn create_verification_request(
        compiler_versions: CompilerVersions,
    ) -> VerificationIncomingRequest {
        VerificationIncomingRequest {
            contract_address: Address::default(),
            source_code_data: SourceCodeData::SolSingleFile("pragma solidity ^0.8.0;".to_string()),
            contract_name: "TestContract".to_string(),
            compiler_versions,
            optimization_used: true,
            optimizer_mode: None,
            constructor_arguments: vec![].into(),
            is_system: false,
            force_evmla: false,
            evm_specific: VerificationEvmSettings::default(),
        }
    }

    #[test]
    fn test_compiler_versions_match() {
        let test_vector = vec![
            // Solc
            (
                "Solc compiler versions should match",
                CompilerVersions::Solc {
                    compiler_solc_version: "0.8.1".to_string(),
                    compiler_zksolc_version: None,
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::Native(vec![0, 8, 1])),
                    ..CborMetadata::default()
                },
                true,
            ),
            (
                "Solc compiler versions shouldn't match",
                CompilerVersions::Solc {
                    compiler_solc_version: "0.8.1".to_string(),
                    compiler_zksolc_version: None,
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::Native(vec![0, 8, 0])),
                    ..CborMetadata::default()
                },
                false,
            ),
            (
                "Solc + zksolc compiler versions should match",
                CompilerVersions::Solc {
                    compiler_solc_version: "zkVM-0.8.24-1.0.1".to_string(),
                    compiler_zksolc_version: Some("v1.5.13".to_string()),
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::ZKsync(
                        "zksolc:1.5.13;solc:0.8.24;llvm:1.0.1".to_string(),
                    )),
                    ..CborMetadata::default()
                },
                true,
            ),
            (
                "Solc + zksolc compiler versions shouldn't match if zk doesn't match",
                CompilerVersions::Solc {
                    compiler_solc_version: "zkVM-0.8.24-1.0.1".to_string(),
                    compiler_zksolc_version: Some("v1.5.12".to_string()),
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::ZKsync(
                        "zksolc:1.5.13;solc:0.8.24;llvm:1.0.1".to_string(),
                    )),
                    ..CborMetadata::default()
                },
                false,
            ),
            (
                "Solc + zksolc compiler versions shouldn't match if solc doesn't match",
                CompilerVersions::Solc {
                    compiler_solc_version: "zkVM-0.8.22-1.0.1".to_string(),
                    compiler_zksolc_version: Some("v1.5.13".to_string()),
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::ZKsync(
                        "zksolc:1.5.13;solc:0.8.24;llvm:1.0.1".to_string(),
                    )),
                    ..CborMetadata::default()
                },
                false,
            ),
            (
                "Solc + zksolc compiler versions shouldn't match if both don't match",
                CompilerVersions::Solc {
                    compiler_solc_version: "zkVM-0.8.22-1.0.1".to_string(),
                    compiler_zksolc_version: Some("v1.5.12".to_string()),
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::ZKsync(
                        "zksolc:1.5.13;solc:0.8.24;llvm:1.0.1".to_string(),
                    )),
                    ..CborMetadata::default()
                },
                false,
            ),
            // Vyper
            (
                "Vyper compiler versions should match",
                CompilerVersions::Vyper {
                    compiler_vyper_version: "0.4.1".to_string(),
                    compiler_zkvyper_version: None,
                },
                CborMetadata {
                    vyper: Some(CborCompilerVersion::Native(vec![0, 4, 1])),
                    ..CborMetadata::default()
                },
                true,
            ),
            (
                "Vyper compiler versions shouldn't match",
                CompilerVersions::Vyper {
                    compiler_vyper_version: "0.4.1".to_string(),
                    compiler_zkvyper_version: None,
                },
                CborMetadata {
                    vyper: Some(CborCompilerVersion::Native(vec![0, 4, 0])),
                    ..CborMetadata::default()
                },
                false,
            ),
            (
                "Vyper + zkvyper compiler versions should match",
                CompilerVersions::Vyper {
                    compiler_vyper_version: "0.4.1".to_string(),
                    compiler_zkvyper_version: Some("v1.5.10".to_string()),
                },
                CborMetadata {
                    vyper: Some(CborCompilerVersion::ZKsync(
                        "zkvyper:1.5.10;vyper:0.4.1".to_string(),
                    )),
                    ..CborMetadata::default()
                },
                true,
            ),
            (
                "Vyper + zkvyper compiler versions shouldn't match if zk doesn't match",
                CompilerVersions::Vyper {
                    compiler_vyper_version: "0.4.1".to_string(),
                    compiler_zkvyper_version: Some("v1.5.10".to_string()),
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::ZKsync(
                        "zkvyper:1.5.9;vyper:0.4.1".to_string(),
                    )),
                    ..CborMetadata::default()
                },
                false,
            ),
            (
                "Vyper + zkvyper compiler versions shouldn't match if vyper doesn't match",
                CompilerVersions::Vyper {
                    compiler_vyper_version: "0.4.1".to_string(),
                    compiler_zkvyper_version: Some("v1.5.10".to_string()),
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::ZKsync(
                        "zkvyper:1.5.10;vyper:0.4.0".to_string(),
                    )),
                    ..CborMetadata::default()
                },
                false,
            ),
            (
                "Vyper + zkvyper compiler versions shouldn't match if both don't match",
                CompilerVersions::Vyper {
                    compiler_vyper_version: "0.4.1".to_string(),
                    compiler_zkvyper_version: Some("v1.5.10".to_string()),
                },
                CborMetadata {
                    solc: Some(CborCompilerVersion::ZKsync(
                        "zkvyper:1.5.9;vyper:0.4.0".to_string(),
                    )),
                    ..CborMetadata::default()
                },
                false,
            ),
        ];

        for (message, compiler_versions, metadata, expected) in test_vector {
            let request = create_verification_request(compiler_versions);
            assert_eq!(
                request.compiler_versions_match(&metadata),
                expected,
                "{}",
                message
            );
        }
    }

    #[test]
    fn with_updated_compiler_versions_solc() {
        let request = create_verification_request(CompilerVersions::Solc {
            compiler_solc_version: "0.8.0".to_string(),
            compiler_zksolc_version: Some("1.0.0".to_string()),
        })
        .with_updated_compiler_versions(&CborMetadata {
            solc: Some(CborCompilerVersion::ZKsync(
                "zksolc:1.5.13;solc:0.8.24;llvm:1.0.1".to_string(),
            )),
            ..CborMetadata::default()
        });

        assert_eq!(
            request.compiler_versions,
            CompilerVersions::Solc {
                compiler_solc_version: "zkVM-0.8.24-1.0.1".to_string(),
                compiler_zksolc_version: Some("v1.5.13".to_string()),
            }
        );
    }

    #[test]
    fn with_updated_compiler_versions_vyper() {
        let request = create_verification_request(CompilerVersions::Vyper {
            compiler_vyper_version: "0.3.0".to_string(),
            compiler_zkvyper_version: Some("1.0.0".to_string()),
        })
        .with_updated_compiler_versions(&CborMetadata {
            vyper: Some(CborCompilerVersion::ZKsync(
                "zkvyper:1.5.10;vyper:0.4.1".to_string(),
            )),
            ..CborMetadata::default()
        });

        assert_eq!(
            request.compiler_versions,
            CompilerVersions::Vyper {
                compiler_vyper_version: "0.4.1".to_string(),
                compiler_zkvyper_version: Some("v1.5.10".to_string()),
            }
        );
    }
}
