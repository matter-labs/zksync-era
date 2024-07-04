use std::{num::NonZeroU64, ops::Div};

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use fraction::{Fraction, ToPrimitive};

/// Represents the base token to ETH conversion ratio at a given point in time.
#[derive(Debug, Clone)]
pub struct BaseTokenRatio {
    pub id: u32,
    pub ratio_timestamp: DateTime<Utc>,
    pub numerator: NonZeroU64,
    pub denominator: NonZeroU64,
    pub used_in_l1: bool,
}

/// Struct to represent API response containing denominator, numerator, and timestamp.
#[derive(Debug, Clone)]
pub struct BaseTokenAPIPrice {
    pub base_token_price: BigDecimal,
    pub eth_price: BigDecimal,
    // Either the timestamp of the quote or the timestamp of the request.
    pub ratio_timestamp: DateTime<Utc>,
}

impl BaseTokenAPIPrice {
    pub fn get_fraction(self) -> anyhow::Result<(NonZeroU64, NonZeroU64)> {
        let rate_fraction = Fraction::from(
            self.base_token_price
                .div(self.eth_price)
                .to_f64()
                .expect("Failed to convert base token price to f64"),
        );

        let numerator = NonZeroU64::new(*rate_fraction.numer().expect("numerator is empty"))
            .expect("numerator is zero");
        let denominator = NonZeroU64::new(*rate_fraction.denom().expect("denominator is empty"))
            .expect("denominator is zero");

        Ok((numerator, denominator))
    }
}
