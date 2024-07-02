use std::{
    ops::{Add, Div},
    sync::Arc,
};

use async_trait::async_trait;
use bigdecimal::{BigDecimal, Zero};
use chrono::Utc;
use tokio::sync::Mutex;
use zksync_types::{base_token_price::BaseTokenAPIPrice, Address};

use crate::PriceAPIClient;

#[derive(Debug)]
struct AverageStrategy {
    pricing_apis: Vec<Arc<Mutex<dyn PriceAPIClient>>>,
}

impl AverageStrategy {
    fn new(pricing_apis: Vec<Arc<Mutex<dyn PriceAPIClient>>>) -> Self {
        return Self { pricing_apis };
    }
}

#[async_trait]
impl PriceAPIClient for AverageStrategy {
    async fn fetch_price(&mut self, token_address: Address) -> anyhow::Result<BaseTokenAPIPrice> {
        let mut success_cnt = 0;
        let mut aggregate_base_token_price = BigDecimal::zero();
        let mut aggregate_eth_price = BigDecimal::zero();
        for p in &mut self.pricing_apis {
            match p.lock().await.fetch_price(token_address).await {
                Ok(x) => {
                    aggregate_base_token_price = aggregate_base_token_price.add(x.base_token_price);
                    aggregate_eth_price = aggregate_eth_price.add(x.eth_price);
                    success_cnt += 1;
                }
                Err(e) => {
                    tracing::info!("Error fetching token price: {}", e)
                }
            }
        }

        if success_cnt == 0 {
            return Err(anyhow::anyhow!("No successful price fetches"));
        }

        Ok(BaseTokenAPIPrice {
            base_token_price: aggregate_base_token_price.div(success_cnt),
            eth_price: aggregate_eth_price.div(success_cnt),
            ratio_timestamp: Utc::now(),
        })
    }
}

#[derive(Debug)]
struct FailOverStrategy {
    pricing_apis: Vec<Arc<Mutex<dyn PriceAPIClient>>>,
}

impl FailOverStrategy {
    fn new(pricing_apis: Vec<Arc<Mutex<dyn PriceAPIClient>>>) -> Self {
        return Self { pricing_apis };
    }
}

#[async_trait]
impl PriceAPIClient for FailOverStrategy {
    async fn fetch_price(&mut self, token_address: Address) -> anyhow::Result<BaseTokenAPIPrice> {
        for p in &mut self.pricing_apis {
            match p.lock().await.fetch_price(token_address).await {
                Ok(x) => return Ok(x),
                Err(e) => {
                    tracing::info!("Error fetching token price: {}", e)
                }
            }
        }
        Err(anyhow::anyhow!("No successful price fetches"))
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc};

    use async_trait::async_trait;
    use bigdecimal::BigDecimal;
    use chrono::{DateTime, Utc};
    use tokio::sync::Mutex;
    use zksync_types::{base_token_price::BaseTokenAPIPrice, Address};

    use crate::{
        strategies::{AverageStrategy, FailOverStrategy},
        PriceAPIClient,
    };

    #[tokio::test]
    async fn test_avg_strategy_happy_day() {
        let api_1 = MockPricingApi::new("1.5".to_string(), "2.5".to_string(), Utc::now());
        let api_2 = MockPricingApi::new("2.5".to_string(), "3.5".to_string(), Utc::now());
        let address = Address::random();

        let mut avg_strategy = AverageStrategy::new(vec![api_1.clone(), api_2.clone()]);
        let result_1 = avg_strategy.fetch_price(address).await.unwrap();
        assert_eq!(result_1.eth_price, BigDecimal::from_str("2").unwrap());
        assert_eq!(
            result_1.base_token_price,
            BigDecimal::from_str("3").unwrap()
        );

        assert_eq!(api_1.lock().await.hit_count, 1);
        assert_eq!(api_2.lock().await.hit_count, 1);
    }

    #[tokio::test]
    async fn test_avg_strategy_should_skip_failed_apis() {
        let api_1 = MockPricingApi::new("1.5".to_string(), "2.5".to_string(), Utc::now());
        let api_2 = MockPricingApi::new("2.5".to_string(), "3.5".to_string(), Utc::now());
        let api_3 = Arc::new(Mutex::new(AlwaysErrorPricingApi { hit_count: 0 }));
        let address = Address::random();

        let mut avg_strategy =
            AverageStrategy::new(vec![api_1.clone(), api_2.clone(), api_3.clone()]);
        let result = avg_strategy.fetch_price(address).await.unwrap();
        assert_eq!(result.eth_price, BigDecimal::from_str("2").unwrap());
        assert_eq!(result.base_token_price, BigDecimal::from_str("3").unwrap());

        assert_eq!(api_1.lock().await.hit_count, 1);
        assert_eq!(api_2.lock().await.hit_count, 1);
        assert_eq!(api_2.lock().await.hit_count, 1);
    }

    #[tokio::test]
    async fn test_avg_strategy_if_all_apis_failed_should_fail_too() {
        let api_1 = Arc::new(Mutex::new(AlwaysErrorPricingApi { hit_count: 0 }));
        let api_2 = Arc::new(Mutex::new(AlwaysErrorPricingApi { hit_count: 0 }));
        let address = Address::random();

        let mut avg_strategy = AverageStrategy::new(vec![api_1.clone(), api_2.clone()]);
        let result = avg_strategy.fetch_price(address).await;
        assert_eq!(
            "No successful price fetches",
            result.err().unwrap().to_string()
        );

        assert_eq!(api_1.lock().await.hit_count, 1);
        assert_eq!(api_2.lock().await.hit_count, 1);
    }

    #[tokio::test]
    async fn test_fail_over_strategy_happy_day() {
        let api_1 = MockPricingApi::new("1.5".to_string(), "2.5".to_string(), Utc::now());
        let api_2 = MockPricingApi::new("2.5".to_string(), "3.5".to_string(), Utc::now());
        let address = Address::random();

        let mut fo_strategy = FailOverStrategy::new(vec![api_1.clone(), api_2.clone()]);
        let result_1 = fo_strategy.fetch_price(address).await.unwrap();
        assert_eq!(result_1.eth_price, BigDecimal::from_str("1.5").unwrap());
        assert_eq!(
            result_1.base_token_price,
            BigDecimal::from_str("2.5").unwrap()
        );

        assert_eq!(api_1.lock().await.hit_count, 1);
        // second API should have never been hit
        assert_eq!(api_2.lock().await.hit_count, 0);
    }

    #[tokio::test]
    async fn test_fail_over_strategy_should_fail_over() {
        let api_1 = Arc::new(Mutex::new(AlwaysErrorPricingApi { hit_count: 0 }));
        let api_2 = MockPricingApi::new("2.5".to_string(), "3.5".to_string(), Utc::now());
        let api_3 = MockPricingApi::new("3.5".to_string(), "4.5".to_string(), Utc::now());
        let address = Address::random();

        let mut fo_strategy =
            FailOverStrategy::new(vec![api_1.clone(), api_2.clone(), api_3.clone()]);
        let result_1 = fo_strategy.fetch_price(address).await.unwrap();
        assert_eq!(result_1.eth_price, BigDecimal::from_str("2.5").unwrap());
        assert_eq!(
            result_1.base_token_price,
            BigDecimal::from_str("3.5").unwrap()
        );

        assert_eq!(api_1.lock().await.hit_count, 1);
        assert_eq!(api_2.lock().await.hit_count, 1);
        // third API should have never been hit
        assert_eq!(api_3.lock().await.hit_count, 0);
    }

    #[tokio::test]
    async fn test_fail_over_strategy_fail_if_all_fail() {
        let api_1 = Arc::new(Mutex::new(AlwaysErrorPricingApi { hit_count: 0 }));
        let api_2 = Arc::new(Mutex::new(AlwaysErrorPricingApi { hit_count: 0 }));
        let address = Address::random();

        let mut fo_strategy = FailOverStrategy::new(vec![api_1.clone(), api_2.clone()]);
        let result = fo_strategy.fetch_price(address).await;
        assert_eq!(
            "No successful price fetches",
            result.err().unwrap().to_string()
        );

        assert_eq!(api_1.lock().await.hit_count, 1);
        assert_eq!(api_2.lock().await.hit_count, 1);
    }

    #[derive(Debug)]
    struct MockPricingApi {
        response: BaseTokenAPIPrice,
        hit_count: i32,
    }

    impl MockPricingApi {
        fn new(
            eth_price: String,
            base_token_price: String,
            ratio_timestamp: DateTime<Utc>,
        ) -> Arc<Mutex<Self>> {
            Arc::new(Mutex::new(MockPricingApi {
                response: BaseTokenAPIPrice {
                    eth_price: BigDecimal::from_str(eth_price.as_str()).unwrap(),
                    base_token_price: BigDecimal::from_str(base_token_price.as_str()).unwrap(),
                    ratio_timestamp,
                },
                hit_count: 0,
            }))
        }
    }

    #[async_trait]
    impl PriceAPIClient for MockPricingApi {
        async fn fetch_price(
            &mut self,
            _token_address: Address,
        ) -> anyhow::Result<BaseTokenAPIPrice> {
            self.hit_count += 1;
            Ok(self.response.clone())
        }
    }

    #[derive(Debug)]
    struct AlwaysErrorPricingApi {
        hit_count: i32,
    }

    #[async_trait]
    impl PriceAPIClient for AlwaysErrorPricingApi {
        async fn fetch_price(
            &mut self,
            _token_address: Address,
        ) -> anyhow::Result<BaseTokenAPIPrice> {
            self.hit_count += 1;
            Err(anyhow::anyhow!("test"))
        }
    }
}
