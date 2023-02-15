//! This module contains the logic used to calculate the price of 1 gas in Wei.

use num::{rational::Ratio, BigUint};

/// Converts any token price in USD into one Wei price per USD.
pub fn token_price_to_wei_price_usd(token_price: &Ratio<BigUint>, decimals: u32) -> Ratio<BigUint> {
    token_price / BigUint::from(10u32).pow(decimals)
}
