use std::ops::Div;
use zksync_types::U256;

pub fn format_eth(value: U256) -> String {
    format!(
        "{:.2}ETH",
        value.div(U256::from(10).pow(U256::from(16))).as_u128() as f64 / 100.0
    )
}

pub fn format_gwei(value: U256) -> String {
    format!(
        "{:.2}GWEI",
        value.div(U256::from(10).pow(U256::from(7))).as_u128() as f64 / 100.0
    )
}
