use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, EnumString, Display, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum TeeType {
    Sgx,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use envy;
    use serde::Deserialize;
    use serde_json;

    use super::TeeType;

    #[derive(Deserialize)]
    struct Config {
        tee_type: TeeType,
    }

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
        assert!(TeeType::from_str("Sgx").is_err());
        assert!(TeeType::from_str("SGX").is_err());
    }

    #[test]
    fn test_display_teetype() {
        assert_eq!(TeeType::Sgx.to_string(), "sgx");
    }

    #[test]
    fn test_envy_teetype() {
        std::env::set_var("TEE_TYPE", "sgx");

        let config = envy::from_env::<Config>().unwrap();
        assert_eq!(config.tee_type, TeeType::Sgx);
    }
}
