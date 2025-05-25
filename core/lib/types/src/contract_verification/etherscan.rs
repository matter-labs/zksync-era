use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::api::{
    CompilerVersions, SourceCodeData, VerificationEvmSettings, VerificationIncomingRequest,
    VerificationRequestStatus,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EtherscanBoolean {
    #[serde(rename = "1")]
    True,
    #[serde(rename = "0")]
    False,
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

/// Etherscan API request. Contains the module name and the payload for the particular action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtherscanRequest {
    pub module: String,
    #[serde(flatten)]
    pub payload: EtherscanRequestPayload,
}

/// Etherscan API request payload. It is used to specify the action to be performed and the data required for that.
/// Only two actions are supported: `verifysourcecode` and `checkverifystatus`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum EtherscanRequestPayload {
    /// Payload for the 'verifysourcecode' action.
    #[serde(rename = "verifysourcecode")]
    VerifySourceCode(EtherscanVerificationRequest),
    /// Payload for the 'checkverifystatus' action.
    #[serde(rename = "checkverifystatus")]
    CheckVerifyStatus { guid: String },
}

/// Response from Etherscan API. For all supported actions, the result is always a string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtherscanResponse {
    pub status: String,
    pub message: String,
    pub result: String,
}

impl EtherscanResponse {
    /// Creates a successful response instance.
    pub fn successful(result: String) -> Self {
        Self {
            status: "1".to_string(),
            message: "OK".to_string(),
            result,
        }
    }

    /// Creates a failed response instance.
    pub fn failed(result: String) -> Self {
        Self {
            status: "0".to_string(),
            message: "NOTOK".to_string(),
            result,
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
        assert_eq!(success_resp.result, "Success Result");

        let failed_resp = EtherscanResponse::failed("Failure Reason".to_string());
        assert_eq!(failed_resp.status, "0");
        assert_eq!(failed_resp.message, "NOTOK");
        assert_eq!(failed_resp.result, "Failure Reason");
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
        assert_eq!(queued_resp.result, "Pending in queue");

        let in_progress_status = VerificationRequestStatus {
            status: "in_progress".to_string(),
            error: None,
            compilation_errors: None,
        };
        let in_progress_resp: EtherscanResponse = in_progress_status.into();
        assert_eq!(in_progress_resp.status, "0");
        assert_eq!(in_progress_resp.result, "Pending in queue");

        let successful_status = VerificationRequestStatus {
            status: "successful".to_string(),
            error: None,
            compilation_errors: None,
        };
        let successful_resp: EtherscanResponse = successful_status.into();
        assert_eq!(successful_resp.status, "1");
        assert_eq!(successful_resp.result, "Pass - Verified");

        let failed_status_with_error = VerificationRequestStatus {
            status: "failed".to_string(),
            error: Some("Compilation error".to_string()),
            compilation_errors: None,
        };
        let failed_resp_with_error: EtherscanResponse = failed_status_with_error.into();
        assert_eq!(failed_resp_with_error.status, "0");
        assert_eq!(
            failed_resp_with_error.result,
            "Fail - Unable to verify. Compilation error"
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
            "Fail - Unable to verify. Unknown verification status: unknown_state."
        );
    }
}
