use std::{fmt, num::ParseIntError, str::FromStr};

use ethers::prelude::U256;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

pub const PACKED_SEMVER_MINOR_OFFSET: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProtocolSemanticVersion {
    pub minor: u16,
    pub patch: u16,
}

impl ProtocolSemanticVersion {
    const MAJOR_VERSION: u8 = 0;

    pub fn new(minor: u16, patch: u16) -> Self {
        Self { minor, patch }
    }

    pub fn pack(&self) -> U256 {
        (U256::from(self.minor) << U256::from(PACKED_SEMVER_MINOR_OFFSET)) | U256::from(self.patch)
    }
}

impl fmt::Display for ProtocolSemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", Self::MAJOR_VERSION, self.minor, self.patch)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseProtocolSemanticVersionError {
    #[error("invalid format")]
    InvalidFormat,
    #[error("non zero major version")]
    NonZeroMajorVersion,
    #[error("{0}")]
    ParseIntError(ParseIntError),
}

impl FromStr for ProtocolSemanticVersion {
    type Err = ParseProtocolSemanticVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(ParseProtocolSemanticVersionError::InvalidFormat);
        }

        let major = parts[0]
            .parse::<u16>()
            .map_err(ParseProtocolSemanticVersionError::ParseIntError)?;
        if major != 0 {
            return Err(ParseProtocolSemanticVersionError::NonZeroMajorVersion);
        }

        let minor = parts[1]
            .parse::<u16>()
            .map_err(ParseProtocolSemanticVersionError::ParseIntError)?;

        let patch = parts[2]
            .parse::<u16>()
            .map_err(ParseProtocolSemanticVersionError::ParseIntError)?;

        Ok(ProtocolSemanticVersion { minor, patch })
    }
}

impl<'de> Deserialize<'de> for ProtocolSemanticVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ProtocolSemanticVersion::from_str(&s).map_err(D::Error::custom)
    }
}

impl Serialize for ProtocolSemanticVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
