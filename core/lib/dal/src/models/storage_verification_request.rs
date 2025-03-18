use chrono::{DateTime, NaiveDateTime, Utc};
use zksync_types::{
    contract_verification::{
        api::{
            CompilerType, CompilerVersions, SourceCodeData, VerificationIncomingRequest,
            VerificationRequest,
        },
        etherscan::EtherscanVerification,
    },
    Address,
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageVerificationRequest {
    pub id: i64,
    pub contract_address: Vec<u8>,
    pub source_code: String,
    pub contract_name: String,
    pub zk_compiler_version: Option<String>,
    pub compiler_version: String,
    pub optimization_used: bool,
    pub optimizer_mode: Option<String>,
    pub constructor_arguments: Vec<u8>,
    pub is_system: bool,
    pub force_evmla: bool,
    pub evm_specific: Option<serde_json::Value>,
}

impl From<StorageVerificationRequest> for VerificationRequest {
    fn from(value: StorageVerificationRequest) -> Self {
        let source_code_data: SourceCodeData = serde_json::from_str(&value.source_code).unwrap();
        let compiler_versions = match source_code_data.compiler_type() {
            CompilerType::Solc => CompilerVersions::Solc {
                compiler_zksolc_version: value.zk_compiler_version,
                compiler_solc_version: value.compiler_version,
            },
            CompilerType::Vyper => CompilerVersions::Vyper {
                compiler_zkvyper_version: value.zk_compiler_version,
                compiler_vyper_version: value.compiler_version,
            },
        };
        let evm_specific = value
            .evm_specific
            .map(|v| {
                serde_json::from_value(v.clone()).unwrap_or_else(|err| {
                    panic!("Cannot deserialize evm specific fields. Error: {err}, value: {v:?}",);
                })
            })
            .unwrap_or_default();
        VerificationRequest {
            id: value.id as usize,
            req: VerificationIncomingRequest {
                contract_address: Address::from_slice(&value.contract_address),
                source_code_data,
                contract_name: value.contract_name,
                compiler_versions,
                optimization_used: value.optimization_used,
                optimizer_mode: value.optimizer_mode,
                constructor_arguments: value.constructor_arguments.into(),
                is_system: value.is_system,
                force_evmla: value.force_evmla,
                evm_specific,
            },
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageEtherscanVerificationRequest {
    pub id: i64,
    pub contract_address: Vec<u8>,
    pub source_code: String,
    pub contract_name: String,
    pub zk_compiler_version: Option<String>,
    pub compiler_version: String,
    pub optimization_used: bool,
    pub optimizer_mode: Option<String>,
    pub constructor_arguments: Vec<u8>,
    pub is_system: bool,
    pub force_evmla: bool,
    pub etherscan_verification_id: Option<String>,
    pub evm_specific: Option<serde_json::Value>,
    pub attempts: i32,
    pub retry_at: Option<NaiveDateTime>,
}
impl From<StorageEtherscanVerificationRequest> for (VerificationRequest, EtherscanVerification) {
    fn from(value: StorageEtherscanVerificationRequest) -> Self {
        let storage_verifier_request = StorageVerificationRequest {
            id: value.id,
            contract_address: value.contract_address,
            source_code: value.source_code,
            contract_name: value.contract_name,
            zk_compiler_version: value.zk_compiler_version,
            compiler_version: value.compiler_version,
            optimization_used: value.optimization_used,
            optimizer_mode: value.optimizer_mode,
            constructor_arguments: value.constructor_arguments,
            is_system: value.is_system,
            force_evmla: value.force_evmla,
            evm_specific: value.evm_specific,
        };
        (
            storage_verifier_request.into(),
            EtherscanVerification {
                etherscan_verification_id: value.etherscan_verification_id,
                attempts: value.attempts,
                retry_at: value
                    .retry_at
                    .map(|t| DateTime::<Utc>::from_naive_utc_and_offset(t, Utc)),
            },
        )
    }
}
