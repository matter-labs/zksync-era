use std::str::FromStr;

use chrono::Utc;
use httpmock::MockServer;
use zksync_types::{base_token_ratio::BaseTokenApiRatio, Address};

use crate::PriceApiClient;

const TIME_TOLERANCE_MS: i64 = 100;
/// Uniswap (UNI)
pub const TEST_TOKEN_ADDRESS: &str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
/// 1UNI = 0.00269ETH
const TEST_TOKEN_PRICE_ETH: f64 = 0.00269;
/// 1ETH = 371.74UNI; When converting gas price from ETH to UNI
/// you need to multiply by this value. Thus, this should be equal to the ratio.
const TEST_BASE_PRICE: f64 = 371.74;
const PRICE_FLOAT_COMPARE_TOLERANCE: f64 = 0.1;

pub(crate) fn approximate_value(api_price: &BaseTokenApiRatio) -> f64 {
    api_price.numerator.get() as f64 / api_price.denominator.get() as f64
}

pub(crate) struct SetupResult {
    pub(crate) client: Box<dyn PriceApiClient>,
}

pub(crate) type SetupFn =
    fn(server: &MockServer, address: Address, base_token_price: f64) -> SetupResult;

pub(crate) async fn happy_day_test(setup: SetupFn) {
    let server = MockServer::start();
    let address_str = TEST_TOKEN_ADDRESS;
    let address = Address::from_str(address_str).unwrap();

    // APIs return token price in ETH (ETH per 1 token)
    let SetupResult { client } = setup(&server, address, TEST_TOKEN_PRICE_ETH);
    let api_price = client.fetch_ratio(address).await.unwrap();

    // we expect the returned ratio to be such that when multiplying gas price in ETH you get gas
    // price in base token. So we expect such ratio X that X Base = 1ETH
    assert!(
        ((api_price.numerator.get() as f64) / (api_price.denominator.get() as f64)
            - TEST_BASE_PRICE)
            .abs()
            < PRICE_FLOAT_COMPARE_TOLERANCE
    );
    assert!((Utc::now() - api_price.ratio_timestamp).num_milliseconds() <= TIME_TOLERANCE_MS);
}

pub(crate) async fn error_test(setup: SetupFn) -> anyhow::Error {
    let server = MockServer::start();
    let address_str = TEST_TOKEN_ADDRESS;
    let address = Address::from_str(address_str).unwrap();

    let SetupResult { client } = setup(&server, address, 1.0);
    let api_price = client.fetch_ratio(address).await;

    assert!(api_price.is_err());
    api_price.err().unwrap()
}
