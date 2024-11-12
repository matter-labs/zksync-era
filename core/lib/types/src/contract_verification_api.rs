use std::{collections::HashMap, fmt};

use chrono::{DateTime, Utc};
use serde::{
    de::{Deserializer, Error, MapAccess, Unexpected, Visitor},
    Deserialize, Serialize,
};

pub use crate::Execute as ExecuteData;
use crate::{web3::Bytes, Address};

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
    pub optimizer_mode: Option<String>,
    #[serde(default)]
    pub constructor_arguments: Bytes,
    #[serde(default, alias = "enableEraVMExtensions")]
    pub is_system: bool,
    #[serde(default)]
    pub force_evmla: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum CompilerType {
    Solc,
    Vyper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompilerVersions {
    #[serde(rename_all = "camelCase")]
    Solc {
        compiler_zksolc_version: String,
        compiler_solc_version: String,
    },
    #[serde(rename_all = "camelCase")]
    Vyper {
        compiler_zkvyper_version: String,
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

    pub fn zk_compiler_version(&self) -> String {
        match self {
            CompilerVersions::Solc {
                compiler_zksolc_version,
                ..
            } => compiler_zksolc_version.clone(),
            CompilerVersions::Vyper {
                compiler_zkvyper_version,
                ..
            } => compiler_zkvyper_version.clone(),
        }
    }

    pub fn compiler_version(&self) -> String {
        match self {
            CompilerVersions::Solc {
                compiler_solc_version,
                ..
            } => compiler_solc_version.clone(),
            CompilerVersions::Vyper {
                compiler_vyper_version,
                ..
            } => compiler_vyper_version.clone(),
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
    pub bytecode: Vec<u8>,
    pub abi: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationInfo {
    pub request: VerificationRequest,
    pub artifacts: CompilationArtifacts,
    pub verified_at: DateTime<Utc>,
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

#[derive(Debug)]
pub enum DeployContractCalldata {
    Deploy(Vec<u8>),
    Ignore,
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::SourceCodeData;

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
}
