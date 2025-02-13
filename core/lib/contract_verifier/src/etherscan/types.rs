use serde::{Deserialize, Serialize};
use zksync_types::contract_verification_api::{
    CompilerVersions, SourceCodeData, VerificationIncomingRequest,
};

use crate::Address;

fn normalize_zksolc_version(version: Option<String>) -> Option<String> {
    match version {
        Some(version) => {
            if version.starts_with("vm-") {
                version.split('-').nth(1).map(|ver| format!("v{}", ver))
            } else {
                Some(version.clone())
            }
        }
        None => None,
    }
}

fn normalize_solc_version(version: String) -> String {
    let mut solc_version = version.replace("zkVM-", "");
    if !solc_version.starts_with("v") {
        solc_version = format!("v{}", solc_version);
    }
    if !solc_version.ends_with("-1.0.1") {
        solc_version = format!("{}-1.0.1", solc_version);
    }
    solc_version
}

/// EtherscanVerificationRequest struct represents the request that is sent to the Etherscan API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EtherscanVerificationRequest {
    #[serde(rename = "contractaddress")]
    pub contract_address: Address,
    pub source_code: String,
    pub code_format: String,
    #[serde(rename = "contractname")]
    pub contract_name: String,
    #[serde(rename = "zksolcVersion", skip_serializing_if = "Option::is_none")]
    compiler_zksolc_version: Option<String>,
    #[serde(rename = "compilerversion")]
    compiler_solc_version: String,
    // solc / zksync
    #[serde(rename = "compilermode")]
    compiler_mode: String,
    pub optimization_used: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimizer_mode: Option<String>,
    #[serde(rename = "constructorArguements")]
    pub constructor_arguments: String,
    pub is_system: bool,
    pub force_evmla: bool,
}
impl From<VerificationIncomingRequest> for EtherscanVerificationRequest {
    fn from(request: VerificationIncomingRequest) -> Self {
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
            // Should never happen as only sol and json code data are supposed to get here
            _ => panic!("Unsupported source code data format"),
        };

        let (compiler_zksolc_version, compiler_solc_version) = match request.compiler_versions {
            CompilerVersions::Solc {
                compiler_zksolc_version,
                compiler_solc_version,
            } => (
                normalize_zksolc_version(compiler_zksolc_version),
                normalize_solc_version(compiler_solc_version),
            ),
            // Should never happen as only sol and json code data are supposed to get here
            _ => panic!("Unsupported compiler version"),
        };

        let compiler_mode = (if compiler_zksolc_version.is_some() {
            "zksync"
        } else {
            "solc"
        })
        .to_string();

        EtherscanVerificationRequest {
            contract_address: request.contract_address,
            code_format,
            source_code,
            contract_name,
            compiler_zksolc_version,
            compiler_solc_version,
            compiler_mode,
            optimization_used: (if request.optimization_used { "1" } else { "0" }).to_string(),
            optimizer_mode: request.optimizer_mode,
            constructor_arguments: hex::encode(&request.constructor_arguments.0),
            is_system: request.is_system,
            force_evmla: request.force_evmla,
        }
    }
}

#[derive(Serialize, Debug)]
pub(crate) struct ApiRequest<'a, T>
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

impl<'a, T> ApiRequest<'a, T>
where
    T: Serialize,
{
    pub fn new(api_key: &'a str, module: ApiModule, action: ApiAction, other: T) -> Self {
        Self {
            api_key,
            module: module.as_str(),
            action: action.as_str(),
            other,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ApiResponseRaw {
    pub status: String,
    pub message: String,
    pub result: String,
}

#[derive(Debug)]
pub(crate) enum ApiModule {
    Contract,
}

impl ApiModule {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiModule::Contract => "contract",
        }
    }
}

#[derive(Debug)]
pub(crate) enum ApiAction {
    GetAbi,
    GetVerificationStatus,
    VerifySourceCode,
}

impl ApiAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiAction::GetAbi => "getabi",
            ApiAction::GetVerificationStatus => "checkverifystatus",
            ApiAction::VerifySourceCode => "verifysourcecode",
        }
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{web3::Bytes, Address};

    use super::*;

    #[test]
    fn test_normalize_zksolc_version_with_vm_prefix() {
        let normalized_version = normalize_zksolc_version(Some("vm-1.5.0-a167aa3".to_string()));
        assert_eq!(normalized_version, Some("v1.5.0".to_string()));
    }

    #[test]
    fn test_normalize_zksolc_version_without_vm_prefix() {
        let normalized_version = normalize_zksolc_version(Some("v1.5.0".to_string()));
        assert_eq!(normalized_version, Some("v1.5.0".to_string()));
    }

    #[test]
    fn test_normalize_zksolc_version_with_none() {
        let normalized_version = normalize_zksolc_version(None);
        assert_eq!(normalized_version, None);
    }

    #[test]
    fn test_normalize_solc_version_with_zkvm_prefix() {
        let normalized_version = normalize_solc_version("zkVM-0.8.16-1.0.1".to_string());
        assert_eq!(normalized_version, "v0.8.16-1.0.1".to_string());
    }

    #[test]
    fn test_normalize_solc_version_without_zkvm_prefix() {
        let normalized_version = normalize_solc_version("v0.8.16".to_string());
        assert_eq!(normalized_version, "v0.8.16-1.0.1".to_string());
    }

    #[test]
    fn test_normalize_solc_version_without_v_prefix() {
        let normalized_version = normalize_solc_version("0.8.16".to_string());
        assert_eq!(normalized_version, "v0.8.16-1.0.1".to_string());
    }

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
        }
    }
    #[test]
    fn test_verification_request_from_solc_single_file_compilation_request() {
        let request = get_default_verification_request();

        let etherscan_request = EtherscanVerificationRequest::from(request);
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
            "v0.8.16-1.0.1".to_string()
        );
        assert_eq!(etherscan_request.compiler_mode, "solc".to_string());
        assert_eq!(etherscan_request.optimization_used, "1".to_string());
        assert_eq!(etherscan_request.optimizer_mode, Some("3".to_string()));
        assert_eq!(
            etherscan_request.constructor_arguments,
            "deadbeef".to_string()
        );
        assert!(etherscan_request.is_system);
        assert!(etherscan_request.force_evmla);
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

        let etherscan_request = EtherscanVerificationRequest::from(request);
        assert_eq!(etherscan_request.contract_address, Address::default());
        assert_eq!(
            etherscan_request.code_format,
            "solidity-single-file".to_string()
        );
        assert_eq!(etherscan_request.source_code, "contract code".to_string());
        assert_eq!(etherscan_request.contract_name, "MyContract".to_string());
        assert_eq!(
            etherscan_request.compiler_zksolc_version,
            Some("v2.0.0".to_string())
        );
        assert_eq!(
            etherscan_request.compiler_solc_version,
            "v0.8.16-1.0.1".to_string()
        );
        assert_eq!(etherscan_request.compiler_mode, "zksync".to_string());
        assert_eq!(etherscan_request.optimization_used, "0".to_string());
        assert_eq!(etherscan_request.optimizer_mode, None);
        assert_eq!(
            etherscan_request.constructor_arguments,
            "deadbeef".to_string()
        );
        assert!(!etherscan_request.is_system);
        assert!(!etherscan_request.force_evmla);
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
                compiler_solc_version: "zkVM-0.8.19-1.0.1".to_string(),
            },
            optimization_used: true,
            optimizer_mode: None,
            constructor_arguments: Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]),
            is_system: false,
            force_evmla: false,
        };

        let etherscan_request = EtherscanVerificationRequest::from(request);
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
            "v0.8.19-1.0.1".to_string()
        );
        assert_eq!(etherscan_request.compiler_mode, "zksync".to_string());
        assert_eq!(etherscan_request.optimization_used, "1".to_string());
        assert_eq!(etherscan_request.optimizer_mode, None);
        assert_eq!(
            etherscan_request.constructor_arguments,
            "deadbeef".to_string()
        );
        assert!(!etherscan_request.is_system);
        assert!(!etherscan_request.force_evmla);
    }
}
