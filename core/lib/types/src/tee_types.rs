use std::fmt;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TeeType {
    Sgx,
}

impl fmt::Display for TeeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TeeType::Sgx => write!(f, "sgx"),
        }
    }
}

/// Representation of a locked batch. Used in DAL to fetch details about the locked batch to
/// determine whether it should be flagged as permanently ignored if it has no corresponding file in
/// the object store for an extended period.
#[derive(Clone, Debug)]
pub struct LockedBatch {
    /// Locked batch number.
    pub l1_batch_number: L1BatchNumber,
    /// The creation time of the job for this batch.
    pub created_at: NaiveDateTime,
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;

    #[test]
    fn test_serialize_teetype() {
        let json_str = "\"sgx\"";
        let tee_type: TeeType = serde_json::from_str(json_str).unwrap();
        assert_eq!(tee_type, TeeType::Sgx);

        for json_str in &["\"Sgx\"", "\"SGX\""] {
            let result: Result<TeeType, _> = serde_json::from_str(json_str);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_deserialize_teetype() {
        let tee_type = TeeType::Sgx;
        let json_str = serde_json::to_string(&tee_type).unwrap();
        assert_eq!(json_str, "\"sgx\"");
    }

    #[test]
    fn test_display_teetype() {
        assert_eq!(TeeType::Sgx.to_string(), "sgx");
    }
}
