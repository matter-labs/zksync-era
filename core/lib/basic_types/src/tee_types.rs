use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

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

impl FromStr for TeeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(&format!("\"{}\"", s.to_lowercase()))
            .map_err(|_| format!("Invalid value for TeeType: {}", s))
    }
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;

    #[test]
    fn test_deserialize_teetype() {
        let json_str = "\"sgx\"";
        let tee_type: TeeType = serde_json::from_str(json_str).unwrap();
        assert_eq!(tee_type, TeeType::Sgx);

        for json_str in &["\"Sgx\"", "\"SGX\""] {
            let result: Result<TeeType, _> = serde_json::from_str(json_str);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_enumstring_teetype() {
        assert_eq!(TeeType::from_str("sgx").unwrap(), TeeType::Sgx);
        assert_eq!(TeeType::from_str("Sgx").unwrap(), TeeType::Sgx);
        assert_eq!(TeeType::from_str("SGX").unwrap(), TeeType::Sgx);
    }

    #[test]
    fn test_display_teetype() {
        assert_eq!(TeeType::Sgx.to_string(), "sgx");
    }
}
