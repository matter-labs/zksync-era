use zksync_types::contract_verification_api::{
    CompilerType, CompilerVersions, SourceCodeData, VerificationIncomingRequest,
    VerificationRequest,
};
use zksync_types::Address;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageVerificationRequest {
    pub id: i64,
    pub contract_address: Vec<u8>,
    pub source_code: String,
    pub contract_name: String,
    pub zk_compiler_version: String,
    pub compiler_version: String,
    pub optimization_used: bool,
    pub optimizer_mode: Option<String>,
    pub constructor_arguments: Vec<u8>,
    pub is_system: bool,
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
            },
        }
    }
}
