use std::ops::Div;

use zksync_types::U256;

pub fn format(value: U256) -> String {
    format!(
        "{:.2} * 10^18",
        value.div(U256::from(10).pow(U256::from(16))).as_u128() as f64 / 100.0
    )
}

pub fn format_gwei(value: U256) -> String {
    format!(
        "{:.2}GWEI",
        value.div(U256::from(10).pow(U256::from(7))).as_u128() as f64 / 100.0
    )
}
