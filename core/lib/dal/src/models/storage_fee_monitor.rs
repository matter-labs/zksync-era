#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageBlockGasData {
    pub number: i64,

    pub commit_gas: Option<i64>,
    pub commit_base_gas_price: Option<i64>,
    pub commit_priority_gas_price: Option<i64>,

    pub prove_gas: Option<i64>,
    pub prove_base_gas_price: Option<i64>,
    pub prove_priority_gas_price: Option<i64>,

    pub execute_gas: Option<i64>,
    pub execute_base_gas_price: Option<i64>,
    pub execute_priority_gas_price: Option<i64>,
}
