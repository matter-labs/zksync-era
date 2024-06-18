use alloy::primitives::U256;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref AMOUNT_FOR_DISTRIBUTION_TO_WALLETS: U256 =
        U256::from(1000000000000000000000_u128);
}

pub const MINIMUM_BALANCE_FOR_WALLET: u128 = 5000000000000000000;
