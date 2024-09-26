use std::str::FromStr;

use chrono::Utc;
use httpmock::MockServer;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

use crate::{utils::get_fraction, PriceAPIClient};

const TIME_TOLERANCE_MS: i64 = 100;

pub(crate) struct SetupResult {
    pub(crate) client: Box<dyn PriceAPIClient>,
}

pub(crate) type SetupFn =
    fn(server: &MockServer, address: Address, base_token_price: f64) -> SetupResult;

pub(crate) async fn happy_day_test(setup: SetupFn) {
    let server = MockServer::start();
    let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984"; //Uniswap (UNI)
    let address = Address::from_str(address_str).unwrap();
    let base_token_price = 198.9;

    let SetupResult { client } = setup(&server, address, base_token_price);
    let api_price = client.fetch_ratio(address).await.unwrap();

    let (numerator, denominator) = get_fraction(base_token_price);

    assert_eq!(
        BaseTokenAPIRatio {
            numerator,
            denominator,
            ratio_timestamp: api_price.ratio_timestamp,
        },
        api_price
    );
    assert!((Utc::now() - api_price.ratio_timestamp).num_milliseconds() <= TIME_TOLERANCE_MS);
}

pub(crate) async fn error_test(setup: SetupFn) -> anyhow::Error {
    let server = MockServer::start();
    let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
    let address = Address::from_str(address_str).unwrap();

    let SetupResult { client } = setup(&server, address, 1.0);
    let api_price = client.fetch_ratio(address).await;

    assert!(api_price.is_err());
    api_price.err().unwrap()
}
