use sqlx::types::{chrono::NaiveDateTime, BigDecimal};
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageOracle {
    pub gas_token_adjust_coefficient: BigDecimal,
    pub gas_token_adjust_coefficient_updated_at: NaiveDateTime,
}
