use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtherscanVerification {
    pub etherscan_verification_id: Option<String>,
    pub attempts: i32,
    pub retry_at: Option<DateTime<Utc>>,
}
