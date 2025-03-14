use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TeeType {
    None,
    Sgx,
    Tdx,
}

impl fmt::Display for TeeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TeeType::None => write!(f, "none"),
            TeeType::Sgx => write!(f, "sgx"),
            TeeType::Tdx => write!(f, "tdx"),
        }
    }
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

        let json_str = "\"tdx\"";
        let tee_type: TeeType = serde_json::from_str(json_str).unwrap();
        assert_eq!(tee_type, TeeType::Tdx);

        for json_str in &["\"Tdx\"", "\"TDX\""] {
            let result: Result<TeeType, _> = serde_json::from_str(json_str);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_deserialize_teetype() {
        let tee_type = TeeType::Sgx;
        let json_str = serde_json::to_string(&tee_type).unwrap();
        assert_eq!(json_str, "\"sgx\"");

        let tee_type = TeeType::Tdx;
        let json_str = serde_json::to_string(&tee_type).unwrap();
        assert_eq!(json_str, "\"tdx\"");
    }

    #[test]
    fn test_display_teetype() {
        assert_eq!(TeeType::Sgx.to_string(), "sgx");
        assert_eq!(TeeType::Tdx.to_string(), "tdx");
    }
}
