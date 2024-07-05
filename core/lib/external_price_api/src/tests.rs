#[cfg(test)]
pub(crate) mod tests {
    use std::{collections::HashMap, str::FromStr};

    use bigdecimal::BigDecimal;
    use chrono::Utc;
    use httpmock::{Mock, MockServer};
    use url::Url;
    use zksync_types::{base_token_ratio::BaseTokenAPIPrice, Address};

    use crate::PriceAPIClient;

    const TIME_TOLERANCE_MS: i64 = 100;

    pub(crate) type SetupFn = fn(
        server: &MockServer,
        api_key: Option<String>,
        address: Address,
        base_token_price: f64,
        eth_price: f64,
    ) -> Box<dyn PriceAPIClient>;

    pub(crate) fn server_url(server: &MockServer) -> Url {
        Url::from_str(server.url("").as_str()).unwrap()
    }

    pub(crate) fn add_mock(
        server: &MockServer,
        method: httpmock::Method,
        path: String,
        query_params: HashMap<String, String>,
        response_status: u16,
        response_body: String,
        auth_header: String,
        api_key: Option<String>,
    ) -> Mock {
        server.mock(|mut when, then| {
            when = when.method(method).path(path);
            if let Some(x) = api_key {
                when = when.header(auth_header, x);
            }
            for (k, v) in &query_params {
                when = when.query_param(k, v);
            }
            then.status(response_status).body(response_body);
        })
    }

    pub(crate) async fn happy_day_test(api_key: Option<String>, setup: SetupFn) {
        let server = MockServer::start();
        let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let address = Address::from_str(address_str).unwrap();
        let base_token_price = 198.9;
        let eth_price = 3000.0;

        let mut client = setup(&server, api_key, address, base_token_price, eth_price);
        let api_price = client.fetch_prices(address).await.unwrap();

        assert_eq!(
            BaseTokenAPIPrice {
                base_token_price: BigDecimal::from_str(&base_token_price.to_string()).unwrap(),
                eth_price: BigDecimal::from_str(&eth_price.to_string()).unwrap(),
                ratio_timestamp: api_price.ratio_timestamp,
            },
            api_price
        );
        assert!((Utc::now() - api_price.ratio_timestamp).num_milliseconds() <= TIME_TOLERANCE_MS);
    }

    pub(crate) async fn no_eth_price_404_test(api_key: Option<String>, setup: SetupFn) {
        let server = MockServer::start();
        let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let address = Address::from_str(address_str).unwrap();
        let client = setup(&server, api_key, address, 1.0, 1.0);
        let api_price = client.fetch_prices(address).await;

        assert!(api_price.is_err());
        assert!(api_price
            .err()
            .unwrap()
            .to_string()
            .starts_with("Http error while fetching token price. Status: 404 Not Found"))
    }

    pub(crate) async fn eth_price_not_found_test(api_key: Option<String>, setup: SetupFn) {
        let server = MockServer::start();
        let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let address = Address::from_str("0x1f9840a85d5af5bf1d1762f925bdaddc4201f984").unwrap();

        let client = setup(&server, api_key, address, 1.0, 1.0);
        let api_price = client
            .fetch_prices(Address::from_str(address_str).unwrap())
            .await;

        assert!(api_price.is_err());
        assert!(api_price
            .err()
            .unwrap()
            .to_string()
            .starts_with("Price not found for token"))
    }

    pub(crate) async fn no_base_token_price_404_test(api_key: Option<String>, setup: SetupFn) {
        let server = MockServer::start();
        let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let address = Address::from_str(address_str).unwrap();

        let client = setup(&server, api_key, address, 1.0, 1.0);
        let api_price = client.fetch_prices(address).await;

        assert!(api_price.is_err());
        assert!(api_price
            .err()
            .unwrap()
            .to_string()
            .starts_with("Http error while fetching token price. Status: 404 Not Found"))
    }

    pub(crate) async fn base_token_price_not_found_test(api_key: Option<String>, setup: SetupFn) {
        let server = MockServer::start();
        let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let address = Address::from_str(address_str).unwrap();

        let client = setup(&server, api_key, address, 1.0, 1.0);
        let api_price = client.fetch_prices(address).await;

        assert!(api_price.is_err());
        assert!(api_price
            .err()
            .unwrap()
            .to_string()
            .starts_with("Price not found for token"))
    }
}
