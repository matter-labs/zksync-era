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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EtherscanCodeFormat {
    #[serde(rename = "solidity-single-file")]
    SingleFile,

    #[default]
    #[serde(rename = "solidity-standard-json-input")]
    StandardJsonInput,
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
    pub constructor_arguments: Bytes,
    #[serde(rename = "contractaddress")]
    pub contract_address: Address,
    #[serde(rename = "contractname")]
    pub contract_name: String,
    #[serde(rename = "compilerversion")]
    pub compiler_version: String,
    #[serde(rename = "zksolcVersion")]
    pub zksolc_version: Option<String>,
    #[serde(rename = "optimizationUsed")]
    pub optimization_used: Option<bool>,
    #[serde(rename = "optimizerMode")]
    pub optimizer_mode: Option<String>,
    pub runs: Option<String>,
    #[serde(rename = "evmversion")]
    pub evm_version: Option<String>,
    #[serde(rename = "compilermode")]
    pub compiler_mode: Option<String>,
    #[serde(default, rename = "isSystem")]
    pub is_system: bool,
    #[serde(default, rename = "forceEvmla")]
    pub force_evmla: bool,
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
            optimization_used: self.optimization_used.unwrap_or(false),
            optimizer_mode: self.optimizer_mode,
            constructor_arguments: self.constructor_arguments,
            is_system: self.is_system,
            force_evmla: self.force_evmla,
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
