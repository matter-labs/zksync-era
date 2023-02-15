// use std::{collections::HashMap, str::FromStr};
//
// use async_trait::async_trait;
// use chrono::{DateTime, Utc};
// use itertools::Itertools;
// use num::{rational::Ratio, BigUint};
// use reqwest::{Client, Url};
// use serde::{Deserialize, Serialize};
//
// use zksync_config::FetcherConfig;
// use zksync_storage::{db_view::DBView, tokens::TokensSchema};
// use zksync_types::{tokens::TokenPrice, Address};
// use zksync_utils::UnsignedRatioSerializeAsDecimal;
//
// use crate::data_fetchers::error::ApiFetchError;
//
// use super::FetcherImpl;
//
// #[derive(Debug, Clone)]
// pub struct CoinMarketCapFetcher {
//     client: Client,
//     addr: Url,
// }
//
// impl CoinMarketCapFetcher {
//     pub fn new(config: &FetcherConfig) -> Self {
//         Self {
//             client: Client::new(),
//             addr: Url::from_str(&config.token_list.url).expect("failed parse One Inch URL"),
//         }
//     }
// }
//
// #[async_trait]
// impl FetcherImpl for CoinMarketCapFetcher {
//     async fn fetch_token_price(
//         &self,
//         token_addrs: &[Address],
//     ) -> Result<HashMap<Address, TokenPrice>, ApiFetchError> {
//         let token_addrs = token_addrs.to_vec();
//
//         let tokens = DBView::with_snapshot(move |snap| {
//             let tokens_list = TokensSchema::new(&*snap).token_list();
//
//             token_addrs
//                 .iter()
//                 .cloned()
//                 .filter_map(|token_addr| {
//                     if let Some(token_symbol) = tokens_list.token_symbol(&token_addr) {
//                         Some((token_addr, token_symbol))
//                     } else {
//                         vlog::warn!(
//                             "Error getting token symbol: token address: {:#x}",
//                             token_addr,
//                         );
//                         None
//                     }
//                 })
//                 .collect::<Vec<_>>()
//         })
//         .await;
//
//         if tokens.is_empty() {
//             return Err(ApiFetchError::Other(
//                 "Failed to identify symbols of tokens by their addresses".to_string(),
//             ));
//         }
//
//         let comma_separated_token_symbols = tokens
//             .iter()
//             .map(|(_, token_symbol)| token_symbol)
//             .join(",");
//
//         let request_url = self
//             .addr
//             .join("/v1/cryptocurrency/quotes/latest")
//             .expect("failed to join URL path");
//
//         let mut api_response = self
//             .client
//             .get(request_url.clone())
//             .query(&[("symbol", comma_separated_token_symbols)])
//             .send()
//             .await
//             .map_err(|err| {
//                 ApiFetchError::Other(format!("Coinmarketcap API request failed: {}", err))
//             })?
//             .json::<CoinMarketCapResponse>()
//             .await
//             .map_err(|err| ApiFetchError::UnexpectedJsonFormat(err.to_string()))?;
//
//         let result = tokens
//             .into_iter()
//             .filter_map(|(token_addr, token_symbol)| {
//                 let token_info = api_response.data.remove(&token_symbol);
//                 let usd_quote = token_info.and_then(|mut token_info| token_info.quote.remove("USD"));
//
//                 if let Some(usd_quote) = usd_quote {
//                     Some((
//                         token_addr,
//                         TokenPrice {
//                             usd_price: usd_quote.price,
//                             last_updated: usd_quote.last_updated,
//                         },
//                     ))
//                 } else {
//                     vlog::warn!(
//                         "Error getting token price from CoinMarketCap: token address: {:#x}, token symbol: {}",
//                         token_addr,
//                         token_symbol,
//                     );
//                     None
//                 }
//             })
//             .collect();
//
//         Ok(result)
//     }
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
// pub struct CoinMarketCapQuote {
//     #[serde(with = "UnsignedRatioSerializeAsDecimal")]
//     pub price: Ratio<BigUint>,
//     pub last_updated: DateTime<Utc>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
// pub struct CoinMarketCapTokenInfo {
//     pub quote: HashMap<String, CoinMarketCapQuote>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct CoinMarketCapResponse {
//     pub data: HashMap<String, CoinMarketCapTokenInfo>,
// }
//
// #[test]
// fn parse_coin_market_cap_response() {
//     let example = r#"{
// "status": {
//     "timestamp": "2020-04-17T04:51:12.012Z",
//     "error_code": 0,
//     "error_message": null,
//     "elapsed": 9,
//     "credit_count": 1,
//     "notice": null
// },
// "data": {
//     "ETH": {
//         "id": 1027,
//         "name": "Ethereum",
//         "symbol": "ETH",
//         "slug": "ethereum",
//         "num_market_pairs": 5153,
//         "date_added": "2015-08-07T00:00:00.000Z",
//         "tags": [
//             "mineable"
//         ],
//         "max_supply": null,
//         "circulating_supply": 110550929.1865,
//         "total_supply": 110550929.1865,
//         "platform": null,
//         "cmc_rank": 2,
//         "last_updated": "2020-04-17T04:50:41.000Z",
//         "quote": {
//             "USD": {
//                 "price": 170.692214992,
//                 "volume_24h": 22515583743.3856,
//                 "percent_change_1h": -0.380817,
//                 "percent_change_24h": 11.5718,
//                 "percent_change_7d": 3.6317,
//                 "market_cap": 18870182972.267426,
//                 "last_updated": "2020-04-17T04:50:41.000Z"
//             }
//         }
//     }
// }
// }"#;
//
//     let resp =
//         serde_json::from_str::<CoinMarketCapResponse>(example).expect("serialization failed");
//     let token_data = resp.data.get("ETH").expect("ETH data not found");
//     let quote = token_data.quote.get("USD").expect("USD not found");
//     assert_eq!(
//         quote.price,
//         UnsignedRatioSerializeAsDecimal::deserialize_from_str_with_dot("170.692214992").unwrap()
//     );
//     assert_eq!(
//         quote.last_updated,
//         DateTime::<Utc>::from_str("2020-04-17T04:50:41.000Z").unwrap()
//     );
// }
