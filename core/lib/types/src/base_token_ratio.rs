use chrono::{DateTime, Utc};

use crate::fee_model::{BaseTokenConversionRatio, ConversionRatio};

/// Represents the base token to ETH conversion ratio at a given point in time.
#[derive(Debug, Clone)]
pub struct BaseTokenRatio {
    pub id: u32,
    pub ratio_timestamp: DateTime<Utc>,
    pub ratio: BaseTokenConversionRatio,
    pub used_in_l1: bool,
}

/// Struct to represent API response containing denominator, numerator, and timestamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseTokenApiRatio {
    pub ratio: ConversionRatio,
    // Either the timestamp of the quote or the timestamp of the request.
    pub ratio_timestamp: DateTime<Utc>,
}

impl BaseTokenApiRatio {
    pub fn identity() -> Self {
        Self {
            ratio: ConversionRatio::default(),
            ratio_timestamp: Utc::now(),
        }
    }

    pub fn reciprocal(&self) -> Self {
        Self {
            ratio: self.ratio.reciprocal(),
            ratio_timestamp: self.ratio_timestamp,
        }
    }
}

impl Default for BaseTokenApiRatio {
    fn default() -> Self {
        Self::identity()
    }
}
