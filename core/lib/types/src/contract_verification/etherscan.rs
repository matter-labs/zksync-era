use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::api::{
    CompilerType, CompilerVersions, SourceCodeData, VerificationEvmSettings,
    VerificationIncomingRequest, VerificationInfo, VerificationRequestStatus,
};
use crate::{web3::Bytes, Address};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtherscanVerification {
    pub etherscan_verification_id: Option<String>,
    pub attempts: i32,
    pub retry_at: Option<DateTime<Utc>>,
}

/// Code format supported by Etherscan API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EtherscanCodeFormat {
    #[serde(rename = "solidity-single-file")]
    SingleFile,

    #[serde(rename = "solidity-standard-json-input")]
    StandardJsonInput,
}

/// It is used to represent boolean values in the API requests. "1" means true and "0" means false.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum EtherscanBoolean {
    #[default]
    #[serde(rename = "0")]
    False,
    #[serde(rename = "1")]
    True,
}

impl EtherscanBoolean {
    /// Converts EtherscanBoolean to a boolean value.
    pub fn to_bool(&self) -> bool {
        match self {
            EtherscanBoolean::True => true,
            EtherscanBoolean::False => false,
        }
    }
}

impl From<bool> for EtherscanBoolean {
    /// Converts a boolean value to EtherscanBoolean.
    fn from(value: bool) -> Self {
        if value {
            EtherscanBoolean::True
        } else {
            EtherscanBoolean::False
        }
    }
}

/// Etherscan verification request. It is used for Etherscan-like API requests and is transformed into
/// `VerificationIncomingRequest` before being sent to the verifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
// This struct is deserialized with serde_urlencoded which doesn't support all serde attributes including serde
// `rename_all`. Therefore, we need to use rename attributes for each field individually.
pub struct EtherscanVerificationRequest {
    #[serde(rename = "codeformat")]
    pub code_format: EtherscanCodeFormat,
    // The struct is deserialized with serde_urlencoded which doesn't support complex enum types and serde tag
    // attribute, that's why the source_code field is of type String and not SourceCodeData.
    #[serde(rename = "sourceCode")]
    pub source_code: String,
    #[serde(default, rename = "constructorArguements")]
    // Bytes deserializer requires leading 0x, so String is used here and then converted to Bytes in the
    // `to_verification_request` method.
    pub constructor_arguments: String,
    #[serde(rename = "contractaddress")]
    pub contract_address: Address,
    #[serde(rename = "contractname")]
    pub contract_name: String,
    #[serde(rename = "compilerversion")]
    pub compiler_version: String,
    #[serde(rename = "zksolcVersion")]
    pub zksolc_version: Option<String>,
    #[serde(rename = "optimizationUsed")]
    pub optimization_used: Option<EtherscanBoolean>,
    #[serde(rename = "optimizerMode")]
    pub optimizer_mode: Option<String>,
    pub runs: Option<String>,
    #[serde(rename = "evmversion")]
    pub evm_version: Option<String>,
    #[serde(rename = "compilermode")]
    pub compiler_mode: Option<String>,
    #[serde(default, rename = "isSystem")]
    pub is_system: Option<EtherscanBoolean>,
    #[serde(default, rename = "forceEvmla")]
    pub force_evmla: Option<EtherscanBoolean>,
}

impl EtherscanVerificationRequest {
    /// Converts the Etherscan verification request to a `VerificationIncomingRequest` which can be processed by the
    /// verifier in a usual way.
    /// Returns result with VerificationIncomingRequest or an error if the source code is not valid JSON.
    pub fn to_verification_request(self) -> Result<VerificationIncomingRequest, anyhow::Error> {
        Ok(VerificationIncomingRequest {
            contract_address: self.contract_address,
            source_code_data: match self.code_format {
                EtherscanCodeFormat::SingleFile => SourceCodeData::SolSingleFile(self.source_code),
                EtherscanCodeFormat::StandardJsonInput => {
                    SourceCodeData::StandardJsonInput(serde_json::from_str(&self.source_code)?)
                }
            },
            contract_name: self.contract_name,
            compiler_versions: CompilerVersions::Solc {
                compiler_solc_version: {
                    let compiler_version = self.compiler_version;
                    if compiler_version.starts_with("zkVM") {
                        // Return as is for zkVM compiler versions
                        compiler_version
                    } else {
                        // Otherwise, extract short solc version from the full version string
                        // e.g. "v0.8.24+commit.e11b9ed9" -> "0.8.24"
                        compiler_version
                            .strip_prefix('v')
                            .unwrap_or(&compiler_version)
                            .split_once('+')
                            .map(|(version, _)| version.to_string())
                            .unwrap_or(compiler_version)
                    }
                },
                compiler_zksolc_version: self.zksolc_version,
            },
            optimization_used: self.optimization_used.map(|x| x.to_bool()).unwrap_or(false),
            optimizer_mode: self.optimizer_mode,
            constructor_arguments: Bytes::from(
                hex::decode(self.constructor_arguments)
                    .map_err(|_| anyhow::anyhow!("Invalid constructor arguments"))?,
            ),
            is_system: self.is_system.map(|x| x.to_bool()).unwrap_or(false),
            force_evmla: self.force_evmla.map(|x| x.to_bool()).unwrap_or(false),
            evm_specific: VerificationEvmSettings {
                evm_version: self.evm_version,
                optimizer_runs: self.runs.map(|x| x.parse().unwrap()),
            },
        })
    }
}

/// Etherscan getsourcecode response.
#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct EtherscanSourceCodeResponse {
    pub source_code: String,
    #[serde(rename = "ABI")]
    pub abi: String,
    pub contract_name: String,
    pub compiler_version: String,
    pub zk_solc_version: String,
    #[serde(default)]
    pub compiler_type: String,
    pub optimization_used: EtherscanBoolean,
    pub runs: String,
    #[serde(default)]
    pub constructor_arguments: String,
    #[serde(rename = "EVMVersion")]
    pub evm_version: String,
    #[serde(default)]
    pub library: String,
    #[serde(default)]
    pub license_type: String,
    pub proxy: EtherscanBoolean,
    #[serde(default)]
    pub implementation: String,
    #[serde(default)]
    pub swarm_source: String,
    #[serde(default)]
    pub similar_match: String,
}

impl From<Option<VerificationInfo>> for EtherscanSourceCodeResponse {
    /// Converts a VerificationInfo to an EtherscanSourceCodeResponse.
    fn from(verification_info: Option<VerificationInfo>) -> Self {
        // Etherscan API returns an empty response if the contract source code is not verified.
        if verification_info.is_none() {
            return EtherscanSourceCodeResponse {
                abi: "Contract source code not verified".to_string(),
                ..Default::default()
            };
        }
        let verification_info = verification_info.unwrap();
        let compiler_type = match verification_info
            .request
            .req
            .compiler_versions
            .compiler_type()
        {
            CompilerType::Solc => "solc",
            CompilerType::Vyper => "vyper",
        };
        Self {
            source_code: serde_json::to_string(&verification_info.request.req.source_code_data)
                .unwrap_or_default(),
            abi: verification_info.artifacts.abi.to_string(),
            contract_name: verification_info.request.req.contract_name,
            compiler_version: verification_info
                .request
                .req
                .compiler_versions
                .compiler_version()
                .to_string(),
            zk_solc_version: verification_info
                .request
                .req
                .compiler_versions
                .zk_compiler_version()
                .unwrap_or_default()
                .to_string(),
            compiler_type: compiler_type.to_string(),
            optimization_used: EtherscanBoolean::from(
                verification_info.request.req.optimization_used,
            ),
            runs: verification_info
                .request
                .req
                .evm_specific
                .optimizer_runs
                .map(|runs| runs.to_string())
                .unwrap_or_default(),
            // Bytes deserializer returns a string with a leading 0x, so we encode the constructor arguments to hex
            // string manually.
            constructor_arguments: hex::encode(
                verification_info.request.req.constructor_arguments.0,
            ),
            evm_version: verification_info
                .request
                .req
                .evm_specific
                .evm_version
                .unwrap_or_default(),
            library: String::default(),
            license_type: String::default(),
            proxy: EtherscanBoolean::False,
            implementation: String::default(),
            swarm_source: String::default(),
            similar_match: String::default(),
        }
    }
}

/// Payload for Etherscan GET requests. It is used to specify the action to be performed and
/// the data required for that.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EtherscanGetPayload {
    GetAbi(Address),
    GetSourceCode(Address),
    CheckVerifyStatus(usize),
}

/// Etherscan API GET request params. Contains the module name and query params for any Etherscan GET action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtherscanGetParams {
    pub module: Option<String>,
    pub action: Option<String>,
    pub address: Option<String>,
    pub guid: Option<String>,
}

impl EtherscanGetParams {
    /// Extracts the address from the Etherscan GET parameters. Returns the same error message as Etherscan
    /// if the address is not valid.
    fn get_address(&self) -> Result<Address, anyhow::Error> {
        if let Ok(address) = Address::from_str(self.address.as_deref().unwrap_or("")) {
            Ok(address)
        } else {
            Err(anyhow::anyhow!("Invalid Address format"))
        }
    }

    /// Converts the Etherscan GET parameters to a payload for the Etherscan API.
    /// Error messages are the same as Etherscan API error messages.
    pub fn get_payload(&self) -> Result<EtherscanGetPayload, anyhow::Error> {
        if self.module != Some("contract".to_string()) {
            return Err(anyhow::anyhow!("Error! Missing Or invalid Module name"));
        }

        if self.action.is_none() {
            return Err(anyhow::anyhow!("Error! Missing Or invalid Action name"));
        }
        let action = self.action.as_deref().unwrap();

        match action {
            "getabi" => Ok(EtherscanGetPayload::GetAbi(self.get_address()?)),
            "getsourcecode" => Ok(EtherscanGetPayload::GetSourceCode(self.get_address()?)),
            "checkverifystatus" => {
                let verification_id = self.guid.clone().unwrap_or_default().parse::<usize>();
                match verification_id {
                    Ok(id) => Ok(EtherscanGetPayload::CheckVerifyStatus(id)),
                    _ => Err(anyhow::anyhow!("Invalid GUID")),
                }
            }
            _ => Err(anyhow::anyhow!("Error! Missing Or invalid Action name")),
        }
    }
}

/// Etherscan API POST request. Contains the module name and the payload for the particular action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtherscanPostRequest {
    pub module: String,
    #[serde(flatten)]
    pub payload: EtherscanPostPayload,
}

/// Etherscan API POST request payload. It is used to specify the action to be performed and the data required for that.
/// Only two actions are supported: `verifysourcecode` and `checkverifystatus`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum EtherscanPostPayload {
    /// Payload for the 'verifysourcecode' action.
    #[serde(rename = "verifysourcecode")]
    VerifySourceCode(EtherscanVerificationRequest),
    /// Payload for the 'checkverifystatus' action.
    #[serde(rename = "checkverifystatus")]
    CheckVerifyStatus { guid: String },
}

/// Etherscan API response result. It can either be a string or a structured response containing the source code.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EtherscanResult {
    String(String),
    SourceCode(EtherscanSourceCodeResponse),
}

/// Response from Etherscan API. For all supported actions, the result is always a string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtherscanResponse {
    pub status: String,
    pub message: String,
    pub result: EtherscanResult,
}

impl EtherscanResponse {
    /// Creates a successful response instance.
    pub fn successful(result: String) -> Self {
        Self {
            status: "1".to_string(),
            message: "OK".to_string(),
            result: EtherscanResult::String(result),
        }
    }

    /// Creates a failed response instance.
    pub fn failed(result: String) -> Self {
        Self {
            status: "0".to_string(),
            message: "NOTOK".to_string(),
            result: EtherscanResult::String(result),
        }
    }
}

impl From<VerificationRequestStatus> for EtherscanResponse {
    /// Converts a `VerificationRequestStatus` to an `EtherscanResponse`.
    fn from(verification_status: VerificationRequestStatus) -> Self {
        match verification_status.status.as_str() {
            "queued" | "in_progress" => EtherscanResponse::failed("Pending in queue".to_string()),
            "successful" => EtherscanResponse::successful("Pass - Verified".to_string()),
            "failed" => EtherscanResponse::failed(format!(
                "Fail - Unable to verify. {}",
                verification_status.error.unwrap_or_default()
            )),
            _ => Self::failed(format!(
                "Fail - Unable to verify. Unknown verification status: {}.",
                verification_status.status
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_verification::api::{CompilationArtifacts, VerificationRequest};

    #[test]
    fn test_etherscan_verification_request_deserialize_single_file() {
        let form = "codeformat=solidity-single-file&sourceCode=contract%20Test%20%7B%7D&constructorArguements=0x&contractaddress=0x1234567890123456789012345678901234567890&contractname=TestContract&compilerversion=v0.8.24%2Bcommit.e11b9ed9&zksolcVersion=v1.5.0&optimizationUsed=1&optimizerMode=z&runs=200&evmversion=london&compilermode=zkevm&isSystem=1&forceEvmla=1";
        let req: EtherscanVerificationRequest = serde_urlencoded::from_str(form).unwrap();

        assert_eq!(req.code_format, EtherscanCodeFormat::SingleFile);
        assert_eq!(req.source_code, "contract Test {}");
        assert_eq!(req.constructor_arguments, "0x");
        assert_eq!(
            req.contract_address,
            "0x1234567890123456789012345678901234567890"
                .parse()
                .unwrap()
        );
        assert_eq!(req.contract_name, "TestContract");
        assert_eq!(req.compiler_version, "v0.8.24+commit.e11b9ed9");
        assert_eq!(req.zksolc_version, Some("v1.5.0".to_string()));
        assert_eq!(req.optimization_used, Some(EtherscanBoolean::True));
        assert_eq!(req.optimizer_mode, Some("z".to_string()));
        assert_eq!(req.runs, Some("200".to_string()));
        assert_eq!(req.evm_version, Some("london".to_string()));
        assert_eq!(req.compiler_mode, Some("zkevm".to_string()));
        assert_eq!(req.is_system, Some(EtherscanBoolean::True));
        assert_eq!(req.force_evmla, Some(EtherscanBoolean::True));
    }

    #[test]
    fn test_etherscan_verification_request_deserialize_json_input() {
        let form = "codeformat=solidity-standard-json-input&sourceCode={language:\"Solidity\"}&constructorArguements=0x&contractaddress=0x1234567890123456789012345678901234567890&contractname=TestContract&compilerversion=v0.8.24%2Bcommit.e11b9ed9&zksolcVersion=v1.5.0&optimizationUsed=1&optimizerMode=z&runs=200&evmversion=london&compilermode=zkevm&isSystem=1&forceEvmla=1";
        let req: EtherscanVerificationRequest = serde_urlencoded::from_str(form).unwrap();

        assert_eq!(req.code_format, EtherscanCodeFormat::StandardJsonInput);
        assert_eq!(req.source_code, "{language:\"Solidity\"}");
        assert_eq!(req.constructor_arguments, "0x");
        assert_eq!(
            req.contract_address,
            "0x1234567890123456789012345678901234567890"
                .parse()
                .unwrap()
        );
        assert_eq!(req.contract_name, "TestContract");
        assert_eq!(req.compiler_version, "v0.8.24+commit.e11b9ed9");
        assert_eq!(req.zksolc_version, Some("v1.5.0".to_string()));
        assert_eq!(req.optimization_used, Some(EtherscanBoolean::True));
        assert_eq!(req.optimizer_mode, Some("z".to_string()));
        assert_eq!(req.runs, Some("200".to_string()));
        assert_eq!(req.evm_version, Some("london".to_string()));
        assert_eq!(req.compiler_mode, Some("zkevm".to_string()));
        assert_eq!(req.is_system, Some(EtherscanBoolean::True));
        assert_eq!(req.force_evmla, Some(EtherscanBoolean::True));
    }

    #[test]
    fn test_to_verification_request_single_file() {
        let etherscan_req = EtherscanVerificationRequest {
            code_format: EtherscanCodeFormat::SingleFile,
            source_code: "contract Test {}".to_string(),
            constructor_arguments: "0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000c48656c6c6f20776f726c64210000000000000000000000000000000000000000".to_string(),
            contract_address: "0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            contract_name: "Test".to_string(),
            compiler_version: "v0.8.24+commit.e11b9ed9".to_string(),
            zksolc_version: Some("v1.5.0".to_string()),
            optimization_used: None,
            optimizer_mode: Some("z".to_string()),
            runs: Some("200".to_string()),
            evm_version: Some("london".to_string()),
            compiler_mode: Some("zkevm".to_string()),
            is_system: None,
            force_evmla: None,
        };

        let args_bytes =
            Bytes::from(hex::decode(etherscan_req.constructor_arguments.clone()).unwrap());
        let verification_req = etherscan_req.to_verification_request().unwrap();

        assert_eq!(
            verification_req.contract_address,
            "0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap()
        );

        if let SourceCodeData::SolSingleFile(source_code_data) = verification_req.source_code_data {
            assert_eq!(source_code_data, "contract Test {}");
        } else {
            panic!("Expected SourceCodeData::SolSingleFile");
        }
        assert_eq!(verification_req.contract_name, "Test");
        assert_eq!(
            verification_req.compiler_versions,
            CompilerVersions::Solc {
                compiler_solc_version: "0.8.24".to_string(),
                compiler_zksolc_version: Some("v1.5.0".to_string())
            }
        );
        assert!(!verification_req.optimization_used);
        assert_eq!(verification_req.optimizer_mode, Some("z".to_string()));
        assert_eq!(verification_req.constructor_arguments, args_bytes);
        assert!(!verification_req.is_system);
        assert!(!verification_req.force_evmla);
        assert_eq!(
            verification_req.evm_specific,
            VerificationEvmSettings {
                evm_version: Some("london".to_string()),
                optimizer_runs: Some(200)
            }
        );
    }

    #[test]
    fn test_to_verification_request_standard_json() {
        let standard_json_input = serde_json::json!({
            "language": "Solidity",
            "sources": {
                "Greeter.sol": {
                    "content": "contract Greeter {}"
                }
            },
            "settings": {
                "outputSelection": {
                    "*": {
                        "*": ["*"]
                    }
                }
            }
        });
        let source_code_str = serde_json::to_string(&standard_json_input).unwrap();

        let etherscan_req = EtherscanVerificationRequest {
            code_format: EtherscanCodeFormat::StandardJsonInput,
            source_code: source_code_str.clone(),
            constructor_arguments: String::default(),
            contract_address: "0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap(),
            contract_name: "TestJson".to_string(),
            compiler_version: "v0.8.24+commit.e11b9ed9".to_string(),
            zksolc_version: None,
            optimization_used: None,
            optimizer_mode: None,
            runs: None,
            evm_version: None,
            compiler_mode: None,
            is_system: Some(EtherscanBoolean::True),
            force_evmla: Some(EtherscanBoolean::True),
        };

        let verification_req = etherscan_req.to_verification_request().unwrap();

        assert_eq!(
            verification_req.contract_address,
            "0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap()
        );
        if let SourceCodeData::StandardJsonInput(source_code_data) =
            verification_req.source_code_data
        {
            assert_eq!(
                source_code_data,
                serde_json::from_str(source_code_str.as_str()).unwrap()
            );
        } else {
            panic!("Expected SourceCodeData::StandardJsonInput");
        }
        assert_eq!(verification_req.contract_name, "TestJson");
        assert_eq!(
            verification_req.compiler_versions,
            CompilerVersions::Solc {
                compiler_solc_version: "0.8.24".to_string(),
                compiler_zksolc_version: None
            }
        );
        assert!(!verification_req.optimization_used);
        assert_eq!(verification_req.optimizer_mode, None);
        assert_eq!(verification_req.constructor_arguments, Bytes::default());
        assert!(verification_req.is_system);
        assert!(verification_req.force_evmla);
        assert_eq!(
            verification_req.evm_specific,
            VerificationEvmSettings {
                evm_version: None,
                optimizer_runs: None
            }
        );
    }

    #[test]
    fn test_to_verification_request_invalid_json() {
        let etherscan_req = EtherscanVerificationRequest {
            code_format: EtherscanCodeFormat::StandardJsonInput,
            source_code: "invalid json".to_string(),
            constructor_arguments: String::default(),
            contract_address: "0x3333333333333333333333333333333333333333"
                .parse()
                .unwrap(),
            contract_name: "InvalidJson".to_string(),
            compiler_version: "v0.8.20+commit.a1b2c3d4".to_string(),
            zksolc_version: None,
            optimization_used: None,
            optimizer_mode: None,
            runs: None,
            evm_version: None,
            compiler_mode: None,
            is_system: None,
            force_evmla: None,
        };

        let result = etherscan_req.to_verification_request();
        assert!(result.is_err());
    }

    #[test]
    fn test_etherscan_response_constructors() {
        let success_resp = EtherscanResponse::successful("Success Result".to_string());
        assert_eq!(success_resp.status, "1");
        assert_eq!(success_resp.message, "OK");
        assert_eq!(
            success_resp.result,
            EtherscanResult::String("Success Result".to_string())
        );

        let failed_resp = EtherscanResponse::failed("Failure Reason".to_string());
        assert_eq!(failed_resp.status, "0");
        assert_eq!(failed_resp.message, "NOTOK");
        assert_eq!(
            failed_resp.result,
            EtherscanResult::String("Failure Reason".to_string())
        );
    }

    #[test]
    fn test_etherscan_response_from_verification_status() {
        let queued_status = VerificationRequestStatus {
            status: "queued".to_string(),
            error: None,
            compilation_errors: None,
        };
        let queued_resp: EtherscanResponse = queued_status.into();
        assert_eq!(queued_resp.status, "0");
        assert_eq!(
            queued_resp.result,
            EtherscanResult::String("Pending in queue".to_string())
        );

        let in_progress_status = VerificationRequestStatus {
            status: "in_progress".to_string(),
            error: None,
            compilation_errors: None,
        };
        let in_progress_resp: EtherscanResponse = in_progress_status.into();
        assert_eq!(in_progress_resp.status, "0");
        assert_eq!(
            in_progress_resp.result,
            EtherscanResult::String("Pending in queue".to_string())
        );

        let successful_status = VerificationRequestStatus {
            status: "successful".to_string(),
            error: None,
            compilation_errors: None,
        };
        let successful_resp: EtherscanResponse = successful_status.into();
        assert_eq!(successful_resp.status, "1");
        assert_eq!(
            successful_resp.result,
            EtherscanResult::String("Pass - Verified".to_string())
        );

        let failed_status_with_error = VerificationRequestStatus {
            status: "failed".to_string(),
            error: Some("Compilation error".to_string()),
            compilation_errors: None,
        };
        let failed_resp_with_error: EtherscanResponse = failed_status_with_error.into();
        assert_eq!(failed_resp_with_error.status, "0");
        assert_eq!(
            failed_resp_with_error.result,
            EtherscanResult::String("Fail - Unable to verify. Compilation error".to_string())
        );

        let unknown_status = VerificationRequestStatus {
            status: "unknown_state".to_string(),
            error: None,
            compilation_errors: None,
        };
        let unknown_resp: EtherscanResponse = unknown_status.into();
        assert_eq!(unknown_resp.status, "0");
        assert_eq!(
            unknown_resp.result,
            EtherscanResult::String(
                "Fail - Unable to verify. Unknown verification status: unknown_state.".to_string()
            )
        );
    }

    #[test]
    fn test_etherscan_source_code_response_from_none() {
        let response: EtherscanSourceCodeResponse = None.into();

        assert_eq!(response.source_code, "");
        assert_eq!(response.abi, "Contract source code not verified");
        assert_eq!(response.contract_name, "");
        assert_eq!(response.compiler_version, "");
        assert_eq!(response.zk_solc_version, "");
        assert_eq!(response.compiler_type, "");
        assert_eq!(response.optimization_used, EtherscanBoolean::False);
        assert_eq!(response.runs, "");
        assert_eq!(response.constructor_arguments, "");
        assert_eq!(response.evm_version, "");
        assert_eq!(response.library, "");
        assert_eq!(response.license_type, "");
        assert_eq!(response.proxy, EtherscanBoolean::False);
        assert_eq!(response.implementation, "");
        assert_eq!(response.swarm_source, "");
        assert_eq!(response.similar_match, "");
    }

    #[test]
    fn test_etherscan_source_code_response_from_verification_info() {
        let verification_request = VerificationRequest {
            id: 1,
            req: VerificationIncomingRequest {
                contract_address: "0x1234567890123456789012345678901234567890"
                    .parse()
                    .unwrap(),
                source_code_data: SourceCodeData::SolSingleFile("contract Test {}".to_string()),
                contract_name: "TestContract".to_string(),
                compiler_versions: CompilerVersions::Solc {
                    compiler_solc_version: "0.8.24".to_string(),
                    compiler_zksolc_version: Some("v1.5.0".to_string()),
                },
                optimization_used: true,
                optimizer_mode: Some("z".to_string()),
                constructor_arguments: Bytes::from(hex::decode("1234").unwrap()),
                is_system: false,
                force_evmla: true,
                evm_specific: VerificationEvmSettings {
                    evm_version: Some("london".to_string()),
                    optimizer_runs: Some(200),
                },
            },
        };

        let verification_artifacts = CompilationArtifacts {
            abi: serde_json::Value::Array(vec![]),
            bytecode: vec![],
            deployed_bytecode: None,
            immutable_refs: Default::default(),
        };

        let verification_info = VerificationInfo {
            request: verification_request,
            artifacts: verification_artifacts,
            verification_problems: [].to_vec(),
            verified_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
        };

        let response: EtherscanSourceCodeResponse = Some(verification_info).into();

        assert!(response.source_code.contains("contract Test {}"));
        assert_eq!(response.abi, "[]");
        assert_eq!(response.contract_name, "TestContract");
        assert_eq!(response.compiler_version, "0.8.24");
        assert_eq!(response.zk_solc_version, "v1.5.0");
        assert_eq!(response.compiler_type, "solc");
        assert_eq!(response.optimization_used, EtherscanBoolean::True);
        assert_eq!(response.runs, "200");
        assert_eq!(response.constructor_arguments, "1234");
        assert_eq!(response.evm_version, "london");
        assert_eq!(response.library, "");
        assert_eq!(response.license_type, "");
        assert_eq!(response.proxy, EtherscanBoolean::False);
        assert_eq!(response.implementation, "");
        assert_eq!(response.swarm_source, "");
        assert_eq!(response.similar_match, "");
    }

    #[test]
    fn test_etherscan_get_params_get_payload_getabi() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("getabi".to_string()),
            address: Some("0x1234567890123456789012345678901234567890".to_string()),
            guid: None,
        };

        let payload = params.get_payload().unwrap();
        match payload {
            EtherscanGetPayload::GetAbi(address) => {
                assert_eq!(
                    address,
                    "0x1234567890123456789012345678901234567890"
                        .parse()
                        .unwrap()
                );
            }
            _ => panic!("Expected GetAbi payload"),
        }
    }

    #[test]
    fn test_etherscan_get_params_get_payload_getsourcecode() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("getsourcecode".to_string()),
            address: Some("0x2222222222222222222222222222222222222222".to_string()),
            guid: None,
        };

        let payload = params.get_payload().unwrap();
        match payload {
            EtherscanGetPayload::GetSourceCode(address) => {
                assert_eq!(
                    address,
                    "0x2222222222222222222222222222222222222222"
                        .parse()
                        .unwrap()
                );
            }
            _ => panic!("Expected GetSourceCode payload"),
        }
    }

    #[test]
    fn test_etherscan_get_params_get_payload_checkverifystatus() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("checkverifystatus".to_string()),
            address: None,
            guid: Some("123456".to_string()),
        };

        let payload = params.get_payload().unwrap();
        match payload {
            EtherscanGetPayload::CheckVerifyStatus(id) => {
                assert_eq!(id, 123456);
            }
            _ => panic!("Expected CheckVerifyStatus payload"),
        }
    }

    #[test]
    fn test_etherscan_get_params_get_payload_invalid_module() {
        let params = EtherscanGetParams {
            module: Some("invalid".to_string()),
            action: Some("getabi".to_string()),
            address: Some("0x1234567890123456789012345678901234567890".to_string()),
            guid: None,
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Error! Missing Or invalid Module name"
        );
    }

    #[test]
    fn test_etherscan_get_params_get_payload_missing_module() {
        let params = EtherscanGetParams {
            module: None,
            action: Some("getabi".to_string()),
            address: Some("0x1234567890123456789012345678901234567890".to_string()),
            guid: None,
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Error! Missing Or invalid Module name"
        );
    }

    #[test]
    fn test_etherscan_get_params_get_payload_missing_action() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: None,
            address: Some("0x1234567890123456789012345678901234567890".to_string()),
            guid: None,
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Error! Missing Or invalid Action name"
        );
    }

    #[test]
    fn test_etherscan_get_params_get_payload_invalid_action() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("invalidaction".to_string()),
            address: Some("0x1234567890123456789012345678901234567890".to_string()),
            guid: None,
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Error! Missing Or invalid Action name"
        );
    }

    #[test]
    fn test_etherscan_get_params_get_payload_invalid_address() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("getabi".to_string()),
            address: Some("invalid_address".to_string()),
            guid: None,
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid Address format");
    }

    #[test]
    fn test_etherscan_get_params_get_payload_missing_address() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("getsourcecode".to_string()),
            address: None,
            guid: None,
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid Address format");
    }

    #[test]
    fn test_etherscan_get_params_get_payload_empty_address() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("getabi".to_string()),
            address: Some("".to_string()),
            guid: None,
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid Address format");
    }

    #[test]
    fn test_etherscan_get_params_get_payload_invalid_id() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("checkverifystatus".to_string()),
            address: None,
            guid: Some("invalid_id".to_string()),
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid GUID");
    }

    #[test]
    fn test_etherscan_get_params_get_payload_missing_id() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("checkverifystatus".to_string()),
            address: None,
            guid: None,
        };

        let result = params.get_payload();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid GUID");
    }

    #[test]
    fn test_etherscan_get_params_get_payload_valid_id() {
        let params = EtherscanGetParams {
            module: Some("contract".to_string()),
            action: Some("checkverifystatus".to_string()),
            address: None,
            guid: Some("0".to_string()),
        };

        let payload = params.get_payload().unwrap();
        match payload {
            EtherscanGetPayload::CheckVerifyStatus(id) => {
                assert_eq!(id, 0);
            }
            _ => panic!("Expected CheckVerifyStatus payload"),
        }
    }
}
