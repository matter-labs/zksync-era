use zksync_types::vm_trace::ContractSourceDebugInfo;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageContractSource {
    pub assembly_code: String,
    pub pc_line_mapping: serde_json::Value,
}

impl From<StorageContractSource> for ContractSourceDebugInfo {
    fn from(source: StorageContractSource) -> ContractSourceDebugInfo {
        ContractSourceDebugInfo {
            assembly_code: source.assembly_code,
            pc_line_mapping: serde_json::from_value(source.pc_line_mapping)
                .expect("invalid pc_line_mapping json in database"),
        }
    }
}
