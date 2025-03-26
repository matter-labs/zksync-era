use serde::{
    de::{DeserializeOwned, Error},
    Deserialize, Serialize,
};
use zksync_types::contract_verification::api::{
    CompilerVersions, SourceCodeData, VerificationEvmSettings, VerificationIncomingRequest,
};

use super::{
    errors::EtherscanError, solc_versions_fetcher::SolcVersionsFetcher,
    utils::normalize_solc_version, ProcessingError, VerifierError,
};
use crate::Address;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum CompilerMode {
    Solc,
    ZkSync,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) enum OptimizationUsed {
    #[serde(rename = "1")]
    Yes,
    #[serde(rename = "0")]
    No,
}

impl From<bool> for OptimizationUsed {
    fn from(used: bool) -> Self {
        if used {
            OptimizationUsed::Yes
        } else {
            OptimizationUsed::No
        }
    }
}

/// EtherscanVerificationRequest struct represents the request that is sent to the Etherscan API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EtherscanVerificationRequest {
    pub contract_address: Address,
    // Unfortunately, we can't use `SourceCodeData` here due to a issue with forms handling in `reqwest`,
    // so we need to serialize it to string right away.
    // See `reqwest` issue for details: https://github.com/seanmonstar/reqwest/issues/274
    pub source_code: String,
    pub code_format: String,
    pub contract_name: String,
    #[serde(rename = "zksolcVersion", skip_serializing_if = "Option::is_none")]
    pub compiler_zksolc_version: Option<String>,
    #[serde(rename = "compilerVersion")]
    pub compiler_solc_version: String,
    pub compiler_mode: CompilerMode,
    pub optimization_used: OptimizationUsed,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimizer_mode: Option<String>,
    pub constructor_arguments: String,
    pub is_system: bool,
    pub force_evmla: bool,
    #[serde(flatten)]
    pub evm_specific: VerificationEvmSettings,
}

impl EtherscanVerificationRequest {
    pub fn from_verification_request(
        request: VerificationIncomingRequest,
        solc_versions_fetcher: &SolcVersionsFetcher,
    ) -> anyhow::Result<Self, VerifierError> {
        let (code_format, source_code, contract_name) = match request.source_code_data {
            SourceCodeData::SolSingleFile(data) => {
                // Extract the actual contract name if the full path is provided
                let contract_name = request
                    .contract_name
                    .rsplit_once(':')
                    .map(|(_, name)| name.to_string())
                    .unwrap_or(request.contract_name.clone());
                ("solidity-single-file".to_string(), data, contract_name)
            }
            SourceCodeData::StandardJsonInput(data) => (
                "solidity-standard-json-input".to_string(),
                serde_json::to_string(&data).unwrap(),
                request.contract_name,
            ),
            _ => return Err(ProcessingError::UnsupportedSourceCodeFormat.into()),
        };

        let (compiler_zksolc_version, compiler_solc_version) = match request.compiler_versions {
            CompilerVersions::Solc {
                compiler_zksolc_version,
                compiler_solc_version,
            } => (
                compiler_zksolc_version,
                normalize_solc_version(compiler_solc_version, solc_versions_fetcher),
            ),
            _ => return Err(ProcessingError::UnsupportedCompilerVersion.into()),
        };

        let compiler_mode = if compiler_zksolc_version.is_some() {
            CompilerMode::ZkSync
        } else {
            CompilerMode::Solc
        };

        Ok(EtherscanVerificationRequest {
            contract_address: request.contract_address,
            code_format,
            source_code,
            contract_name,
            compiler_zksolc_version,
            compiler_solc_version,
            compiler_mode,
            optimization_used: request.optimization_used.into(),
            optimizer_mode: request.optimizer_mode,
            constructor_arguments: hex::encode(&request.constructor_arguments.0),
            is_system: request.is_system,
            force_evmla: request.force_evmla,
            evm_specific: request.evm_specific,
        })
    }
}

#[derive(Serialize, Debug)]
pub(super) struct EtherscanRequest<'a, T>
where
    T: Serialize,
{
    #[serde(rename = "apikey")]
    api_key: &'a str,
    module: &'static str,
    action: &'static str,
    #[serde(flatten)]
    other: T,
}

impl<'a, T> EtherscanRequest<'a, T>
where
    T: Serialize,
{
    pub fn new(
        api_key: &'a str,
        module: EtherscanModule,
        action: EtherscanAction,
        other: T,
    ) -> Self {
        Self {
            api_key,
            module: module.as_str(),
            action: action.as_str(),
            other,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct RawEtherscanResponse {
    pub status: String,
    pub message: String,
    pub result: serde_json::Value,
}

impl RawEtherscanResponse {
    pub(super) fn error_message(&self) -> EtherscanError {
        match self.result.as_str() {
            Some(result) => {
                let result_lower = result.to_lowercase();
                if result_lower.contains("not verified") {
                    return EtherscanError::ContractNotVerified;
                }
                if result_lower.contains("pending") {
                    return EtherscanError::VerificationPending;
                }
                if result_lower.contains("unable to locate contractcode") {
                    return EtherscanError::ContractBytecodeNotAvailable;
                }
                // There is a number of daily limit in between the checked values. I don't want to rely on the exact
                // number as it is a subject to change.
                if result_lower.starts_with("daily limit")
                    && result.ends_with("source code submissions reached")
                {
                    return EtherscanError::DailyVerificationRequestsLimitExceeded;
                }
                if result_lower.contains("already verified") {
                    return EtherscanError::ContractAlreadyVerified;
                }
                if result_lower.contains("rate limit reached") {
                    return EtherscanError::RateLimitExceeded;
                }
                // Error message can be "Invalid API Key" or "Missing/Invalid API Key"
                if result_lower.contains("invalid api key") {
                    return EtherscanError::InvalidApiKey;
                }
                // Page not found error is checked both by 404 status and by the message
                if result_lower.contains("page not found") {
                    return EtherscanError::PageNotFound;
                }
                EtherscanError::ErrorResponse {
                    message: self.message.clone(),
                    result: result.to_string(),
                }
            }
            None => EtherscanError::Serde {
                error: serde_json::Error::custom(
                    "Error deserializing an EtherscanError. Result is not a string.",
                ),
                content: self.message.clone(),
            },
        }
    }

    pub(super) fn deserialize_result<T: DeserializeOwned>(self) -> Result<T, EtherscanError> {
        match serde_json::from_value(self.result) {
            Ok(result) => Ok(result),
            Err(e) => Err(EtherscanError::Serde {
                error: e,
                content: self.message,
            }),
        }
    }
}

#[derive(Debug)]
pub(super) enum EtherscanModule {
    Contract,
}

impl EtherscanModule {
    pub fn as_str(&self) -> &'static str {
        match self {
            EtherscanModule::Contract => "contract",
        }
    }
}

#[derive(Debug)]
pub(super) enum EtherscanAction {
    GetAbi,
    GetVerificationStatus,
    VerifySourceCode,
}

impl EtherscanAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            EtherscanAction::GetAbi => "getabi",
            EtherscanAction::GetVerificationStatus => "checkverifystatus",
            EtherscanAction::VerifySourceCode => "verifysourcecode",
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json;
    use zksync_types::{
        contract_verification::api::{CompilerVersions, SourceCodeData},
        web3::Bytes,
        Address,
    };

    use super::*;

    fn get_default_verification_request() -> VerificationIncomingRequest {
        VerificationIncomingRequest {
            contract_address: Address::default(),
            contract_name: "MyContract".to_string(),
            source_code_data: SourceCodeData::SolSingleFile("contract code".to_string()),
            compiler_versions: CompilerVersions::Solc {
                compiler_zksolc_version: None,
                compiler_solc_version: "0.8.16".to_string(),
            },
            optimization_used: true,
            optimizer_mode: Some("3".to_string()),
            constructor_arguments: Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]),
            is_system: true,
            force_evmla: true,
            evm_specific: VerificationEvmSettings {
                evm_version: Some("evm version".to_string()),
                optimizer_runs: Some(200),
            },
        }
    }
    #[test]
    fn test_verification_request_from_solc_single_file_compilation_request() {
        let request = get_default_verification_request();
        let etherscan_request = EtherscanVerificationRequest::from_verification_request(
            request.clone(),
            &SolcVersionsFetcher::new(),
        )
        .unwrap();
        assert_eq!(etherscan_request.contract_address, Address::default());
        assert_eq!(
            etherscan_request.code_format,
            "solidity-single-file".to_string()
        );
        assert_eq!(etherscan_request.source_code, "contract code".to_string());
        assert_eq!(etherscan_request.contract_name, "MyContract".to_string());
        assert_eq!(etherscan_request.compiler_zksolc_version, None);
        assert_eq!(
            etherscan_request.compiler_solc_version,
            "v0.8.16+commit.07a7930e".to_string()
        );
        assert_eq!(etherscan_request.compiler_mode, CompilerMode::Solc);
        assert_eq!(etherscan_request.optimization_used, OptimizationUsed::Yes);
        assert_eq!(etherscan_request.optimizer_mode, Some("3".to_string()));
        assert_eq!(
            etherscan_request.constructor_arguments,
            "deadbeef".to_string()
        );
        assert!(etherscan_request.is_system);
        assert!(etherscan_request.force_evmla);
        assert_eq!(etherscan_request.evm_specific, request.evm_specific);
    }

    #[test]
    fn test_verification_request_from_zksolc_single_file_compilation_request() {
        let mut request = get_default_verification_request();

        request.is_system = false;
        request.force_evmla = false;
        request.optimization_used = false;
        request.optimizer_mode = None;
        request.contract_name = "MyContract".to_string();
        request.compiler_versions = CompilerVersions::Solc {
            compiler_zksolc_version: Some("vm-2.0.0-abcedf".to_string()),
            compiler_solc_version: "0.8.16".to_string(),
        };

        let etherscan_request = EtherscanVerificationRequest::from_verification_request(
            request.clone(),
            &SolcVersionsFetcher::new(),
        )
        .unwrap();
        assert_eq!(etherscan_request.contract_address, Address::default());
        assert_eq!(
            etherscan_request.code_format,
            "solidity-single-file".to_string()
        );
        assert_eq!(etherscan_request.source_code, "contract code".to_string());
        assert_eq!(etherscan_request.contract_name, "MyContract".to_string());
        assert_eq!(
            etherscan_request.compiler_zksolc_version,
            Some("vm-2.0.0-abcedf".to_string())
        );
        assert_eq!(
            etherscan_request.compiler_solc_version,
            "v0.8.16+commit.07a7930e".to_string()
        );
        assert_eq!(etherscan_request.compiler_mode, CompilerMode::ZkSync);
        assert_eq!(etherscan_request.optimization_used, OptimizationUsed::No);
        assert_eq!(etherscan_request.optimizer_mode, None);
        assert_eq!(
            etherscan_request.constructor_arguments,
            "deadbeef".to_string()
        );
        assert!(!etherscan_request.is_system);
        assert!(!etherscan_request.force_evmla);
        assert_eq!(etherscan_request.evm_specific, request.evm_specific);
    }

    #[test]
    fn test_from_solc_standard_json_input() {
        let stand_json_input_str =
            r#"{"codeFormat": "solidity-standard-json-input", "sourceCode": {}}"#;
        let stand_json_input_result =
            serde_json::from_str::<SourceCodeData>(stand_json_input_str).unwrap();

        let request = VerificationIncomingRequest {
            contract_address: Address::default(),
            contract_name: "contracts/StandardJsonContract.sol:StandardJsonContract".to_string(),
            source_code_data: stand_json_input_result,
            compiler_versions: CompilerVersions::Solc {
                compiler_zksolc_version: Some("v1.3.18".to_string()),
                compiler_solc_version: "zkVM-0.8.19-1.0.0".to_string(),
            },
            optimization_used: true,
            optimizer_mode: None,
            constructor_arguments: Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]),
            is_system: false,
            force_evmla: false,
            evm_specific: VerificationEvmSettings {
                evm_version: Some("evm version".to_string()),
                optimizer_runs: Some(200),
            },
        };

        let etherscan_request = EtherscanVerificationRequest::from_verification_request(
            request.clone(),
            &SolcVersionsFetcher::new(),
        )
        .unwrap();
        assert_eq!(etherscan_request.contract_address, Address::default());
        assert_eq!(
            etherscan_request.code_format,
            "solidity-standard-json-input".to_string()
        );
        assert_eq!(etherscan_request.source_code, "{}".to_string());
        assert_eq!(
            etherscan_request.contract_name,
            "contracts/StandardJsonContract.sol:StandardJsonContract".to_string()
        );
        assert_eq!(
            etherscan_request.compiler_zksolc_version,
            Some("v1.3.18".to_string())
        );
        assert_eq!(
            etherscan_request.compiler_solc_version,
            "v0.8.19-1.0.0".to_string()
        );
        assert_eq!(etherscan_request.compiler_mode, CompilerMode::ZkSync);
        assert_eq!(etherscan_request.optimization_used, OptimizationUsed::Yes);
        assert_eq!(etherscan_request.optimizer_mode, None);
        assert_eq!(
            etherscan_request.constructor_arguments,
            "deadbeef".to_string()
        );
        assert!(!etherscan_request.is_system);
        assert!(!etherscan_request.force_evmla);
        assert_eq!(etherscan_request.evm_specific, request.evm_specific);
    }

    #[test]
    fn test_unsupported_source_code_format() {
        let mut request = super::tests::get_default_verification_request();
        let unsupported_source: SourceCodeData = SourceCodeData::YulSingleFile("dummy".to_string());
        request.source_code_data = unsupported_source;

        let result = EtherscanVerificationRequest::from_verification_request(
            request,
            &SolcVersionsFetcher::new(),
        );
        assert!(matches!(
            result,
            Err(VerifierError::ProcessingError(
                ProcessingError::UnsupportedSourceCodeFormat
            ))
        ));
    }

    #[test]
    fn test_unsupported_compiler_version() {
        let mut request = super::tests::get_default_verification_request();
        let unsupported_compiler: CompilerVersions = CompilerVersions::Vyper {
            compiler_zkvyper_version: Some("compiler_zkvyper_version".to_string()),
            compiler_vyper_version: "compiler_vyper_version".to_string(),
        };
        request.compiler_versions = unsupported_compiler;

        let result = EtherscanVerificationRequest::from_verification_request(
            request,
            &SolcVersionsFetcher::new(),
        );
        assert!(matches!(
            result,
            Err(VerifierError::ProcessingError(
                ProcessingError::UnsupportedCompilerVersion
            ))
        ));
    }

    #[test]
    fn test_raw_etherscan_response_error_message() {
        let cases = vec![
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String(
                        "Contract source code not verified".to_string(),
                    ),
                },
                EtherscanError::ContractNotVerified,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String(
                        "Contract source code already verified".to_string(),
                    ),
                },
                EtherscanError::ContractAlreadyVerified,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String("Already Verified".to_string()),
                },
                EtherscanError::ContractAlreadyVerified,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String("Pending in queue".to_string()),
                },
                EtherscanError::VerificationPending,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String("Unable to locate ContractCode at 0x0000000000000000000000000000000000000000".to_string()),
                },
                EtherscanError::ContractBytecodeNotAvailable,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String(
                        "Daily limit of 100 source code submissions reached".to_string(),
                    ),
                },
                EtherscanError::DailyVerificationRequestsLimitExceeded,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String("Max rate limit reached".to_string()),
                },
                EtherscanError::RateLimitExceeded,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String("Invalid API Key".to_string()),
                },
                EtherscanError::InvalidApiKey,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String("Missing/Invalid API Key".to_string()),
                },
                EtherscanError::InvalidApiKey,
            ),
            (
                RawEtherscanResponse {
                    status: "0".to_string(),
                    message: "NOTOK".to_string(),
                    result: serde_json::Value::String("Unknown error".to_string()),
                },
                EtherscanError::ErrorResponse {
                    message: "NOTOK".to_string(),
                    result: "Unknown error".to_string(),
                },
            ),
        ];

        for (response, expected_error) in cases {
            match (response.error_message(), expected_error) {
                (EtherscanError::ContractNotVerified, EtherscanError::ContractNotVerified) => {}
                (
                    EtherscanError::ContractAlreadyVerified,
                    EtherscanError::ContractAlreadyVerified,
                ) => {}
                (EtherscanError::VerificationPending, EtherscanError::VerificationPending) => {}
                (
                    EtherscanError::DailyVerificationRequestsLimitExceeded,
                    EtherscanError::DailyVerificationRequestsLimitExceeded,
                ) => {}
                (
                    EtherscanError::ContractBytecodeNotAvailable,
                    EtherscanError::ContractBytecodeNotAvailable,
                ) => {}
                (EtherscanError::RateLimitExceeded, EtherscanError::RateLimitExceeded) => {}
                (EtherscanError::InvalidApiKey, EtherscanError::InvalidApiKey) => {}
                (
                    EtherscanError::ErrorResponse {
                        message: actual_msg,
                        result: actual_res,
                    },
                    EtherscanError::ErrorResponse {
                        message: expected_msg,
                        result: expected_res,
                    },
                ) => {
                    assert_eq!(actual_msg, expected_msg);
                    assert_eq!(actual_res, expected_res);
                }
                (actual, expected) => {
                    panic!("Unexpected error variant.\nActual: {actual:?}\nExpected: {expected:?}");
                }
            }
        }
    }

    #[test]
    fn test_raw_etherscan_response_error_message_non_string_result() {
        let response = RawEtherscanResponse {
            status: "0".to_string(),
            message: "Error message".to_string(),
            result: serde_json::Value::Object(serde_json::Map::new()),
        };
        assert!(matches!(
            response.error_message(),
            EtherscanError::Serde { content, .. }
            if content == "Error message"
        ));
    }

    #[test]
    fn test_raw_etherscan_response_deserialize_result() {
        // Test successful deserialization
        let response = RawEtherscanResponse {
            status: "1".to_string(),
            message: "OK".to_string(),
            result: serde_json::Value::String("success".to_string()),
        };
        let result: Result<String, _> = response.deserialize_result();
        assert_eq!(result.unwrap(), "success");

        // Test failed deserialization
        let response = RawEtherscanResponse {
            status: "1".to_string(),
            message: "message".to_string(),
            // Invalid value for String
            result: serde_json::Value::Object(serde_json::Map::new()),
        };
        let result: Result<String, _> = response.deserialize_result();
        assert!(matches!(
            result,
            Err(EtherscanError::Serde { content, .. })
            if content == "message"
        ));
    }
}
