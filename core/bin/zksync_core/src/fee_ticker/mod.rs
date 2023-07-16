//! This module defines the price components of L2 transactions.

use core::fmt::Debug;

use bigdecimal::BigDecimal;
use num::{rational::Ratio, BigUint};
use vm::vm_with_bootloader::base_fee_to_gas_per_pubdata;
use zksync_types::Address;
use zksync_utils::ratio_to_big_decimal_normalized;

use self::error::TickerError;
use zksync_dal::tokens_web3_dal::TokensWeb3Dal;

pub mod error;
mod gas_price;
pub mod types;

/// Amount of possible symbols after the decimal dot in the USD.
/// Used to convert `Ratio<BigUint>` to `BigDecimal`.
pub const USD_PRECISION: usize = 100;

/// Minimum amount of symbols after the decimal dot in the USD.
/// Used to convert `Ratio<BigUint>` to `BigDecimal`.
pub const MIN_PRECISION: usize = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum TokenPriceRequestType {
    USDForOneWei,
    USDForOneToken,
}

#[derive(Debug, Default)]
pub struct FeeTicker;

impl FeeTicker {
    /// Returns the token price in USD.
    pub async fn get_l2_token_price(
        tokens_web3_dal: &mut TokensWeb3Dal<'_, '_>,
        request_type: TokenPriceRequestType,
        l2_token_addr: &Address,
    ) -> Result<BigDecimal, TickerError> {
        Self::get_l2_token_price_inner(tokens_web3_dal, request_type, l2_token_addr)
            .await
            .map(|final_price| {
                ratio_to_big_decimal_normalized(&final_price, USD_PRECISION, MIN_PRECISION)
            })
    }

    /// Returns the acceptable `gas_per_pubdata_byte` based on the current gas price.
    pub fn gas_per_pubdata_byte(gas_price_wei: u64, base_fee: u64) -> u32 {
        base_fee_to_gas_per_pubdata(gas_price_wei, base_fee) as u32
    }

    async fn get_l2_token_price_inner(
        tokens_web3_dal: &mut TokensWeb3Dal<'_, '_>,
        request_type: TokenPriceRequestType,
        l2_token_addr: &Address,
    ) -> Result<Ratio<BigUint>, TickerError> {
        let token_price = tokens_web3_dal
            .get_token_price(l2_token_addr)
            .await
            .map_err(|_| TickerError::InternalError)?
            .ok_or(TickerError::PriceNotTracked(*l2_token_addr))?
            .usd_price;

        let final_price = match request_type {
            TokenPriceRequestType::USDForOneToken => token_price,
            TokenPriceRequestType::USDForOneWei => {
                let token_metadata = tokens_web3_dal
                    .get_token_metadata(l2_token_addr)
                    .await
                    .map_err(|_| TickerError::InternalError)?
                    .ok_or_else(|| {
                        // It's kinda not OK that we have a price for token, but no metadata.
                        // Not a reason for a panic, but surely highest possible report level.
                        vlog::error!(
                            "Token {:x} has price, but no stored metadata",
                            l2_token_addr
                        );
                        TickerError::PriceNotTracked(*l2_token_addr)
                    })?;

                gas_price::token_price_to_wei_price_usd(
                    &token_price,
                    token_metadata.decimals as u32,
                )
            }
        };

        Ok(final_price)
    }
}
