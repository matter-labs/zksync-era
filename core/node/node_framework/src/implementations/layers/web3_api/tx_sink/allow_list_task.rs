use std::{collections::HashSet, time::Duration};

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::time::interval;
use zksync_node_api_server::tx_sender::shared_allow_list::SharedAllowList;
use zksync_types::Address;

use crate::{
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
};

#[derive(Debug)]
pub struct AllowListTask {
    url: String,
    refresh_interval: Duration,
    allowlist: SharedAllowList,
}

#[derive(Debug, Deserialize)]
struct WhitelistResponse {
    addresses: Vec<Address>,
}

impl AllowListTask {
    pub fn new(url: String, refresh_interval: Duration, allowlist: SharedAllowList) -> Self {
        Self {
            url,
            refresh_interval,
            allowlist,
        }
    }

    async fn fetch(&self) -> anyhow::Result<HashSet<Address>> {
        let client = Client::new();
        let response = client.get(&self.url).send().await?.error_for_status()?;
        let parsed: WhitelistResponse = response.json().await?;
        Ok(parsed.addresses.into_iter().collect())
    }
}

#[async_trait]
impl Task for AllowListTask {
    fn id(&self) -> TaskId {
        "api_allowlist_task".into()
    }

    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let mut ticker = interval(self.refresh_interval);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if *stop_receiver.0.borrow_and_update() {
                        tracing::info!("AllowListTask received shutdown signal");
                        break;
                    }

                    match self.fetch().await {
                        Ok(new_list) => {
                            let writer = self.allowlist.writer();
                            let mut lock = writer.write().await;
                            *lock = new_list;
                            tracing::info!("Allowlist updated. {} entries loaded.", lock.len());
                        }
                        Err(err) => {
                            tracing::warn!("Failed to refresh allowlist: {:?}", err);
                        }
                    }

                }

                _ = stop_receiver.0.changed() => {
                    if *stop_receiver.0.borrow() {
                        tracing::info!("AllowListTask received shutdown signal (alt path)");
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}
