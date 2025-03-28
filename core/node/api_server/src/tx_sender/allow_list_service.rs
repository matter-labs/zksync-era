use std::{collections::HashSet, sync::Arc};

use reqwest::Client;
use serde::Deserialize;
use tokio::{
    sync::RwLock,
    time::{interval, Duration},
};
use zksync_types::Address;

#[derive(Debug, Deserialize)]
struct Whitelist {
    addresses: Vec<Address>,
}

#[derive(Clone, Debug)]
pub struct AllowListService {
    allowed_addresses: Arc<RwLock<HashSet<Address>>>,
}

impl AllowListService {
    pub fn new(url: String, refresh_interval: Duration) -> Self {
        let allowed_addresses = Arc::new(RwLock::new(HashSet::new()));
        let addresses_clone = allowed_addresses.clone();

        tokio::spawn(async move {
            let client = Client::new();
            let mut interval = interval(refresh_interval);

            loop {
                interval.tick().await;
                match client.get(&url).send().await {
                    Ok(response) => match response.json::<Whitelist>().await {
                        Ok(data) => {
                            let mut lock = addresses_clone.write().await;
                            lock.clear();
                            lock.extend(data.addresses);
                            tracing::info!("Whitelist updated. {} entries loaded.", lock.len());
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse whitelist JSON: {:?}", e);
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to fetch whitelist: {:?}", e);
                    }
                }
            }
        });

        Self { allowed_addresses }
    }

    pub async fn is_address_allowed(&self, address: &Address) -> bool {
        self.allowed_addresses.read().await.contains(address)
    }
}
