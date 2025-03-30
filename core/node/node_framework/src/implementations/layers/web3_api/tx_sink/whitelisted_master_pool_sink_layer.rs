use std::{collections::HashSet, time::Duration};

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::time::interval;
use zksync_config::configs::api::DeploymentAllowlist;
use zksync_node_api_server::tx_sender::{
    master_pool_sink::MasterPoolSink, shared_allow_list::SharedAllowList,
    whitelisted_deploy_pool_sink::WhitelistedDeployPoolSink,
};
use zksync_types::Address;

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        web3_api::TxSinkResource,
    },
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`WhitelistedDeployPoolSink`] that wraps a `MasterPoolSink` and enables allowlist filtering.
pub struct WhitelistedMasterPoolSinkLayer {
    pub deployment_allowlist: DeploymentAllowlist,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub tx_sink: TxSinkResource,
    #[context(task)]
    pub allow_list_task: AllowListTask,
}

#[async_trait]
impl WiringLayer for WhitelistedMasterPoolSinkLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "whitelisted_master_pool_sink_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let master_pool_sink = MasterPoolSink::new(pool);

        let shared_allowlist = SharedAllowList::new();

        let task = AllowListTask::new(
            self.deployment_allowlist
                .http_file_url()
                .expect("DeploymentAllowlist must contain a URL")
                .to_string(),
            self.deployment_allowlist.refresh_interval(),
            shared_allowlist.clone(),
        );

        let tx_sink = WhitelistedDeployPoolSink::new(master_pool_sink, shared_allowlist).into();

        Ok(Output {
            tx_sink,
            allow_list_task: task,
        })
    }
}

/// Task that periodically fetches and updates the allowlist from a remote HTTP source.
#[derive(Debug)]
pub struct AllowListTask {
    url: String,
    refresh_interval: Duration,
    allowlist: SharedAllowList,
    client: Client,
    etag: tokio::sync::Mutex<Option<String>>,
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
            client: Client::new(),
            etag: tokio::sync::Mutex::new(None),
        }
    }

    async fn fetch(&self) -> anyhow::Result<Option<HashSet<Address>>> {
        let etag_header = self.etag.lock().await.clone();
        let response = self
            .client
            .get(&self.url)
            .timeout(Duration::from_secs(5))
            .header("If-None-Match", etag_header.unwrap_or_default())
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            tracing::debug!("Allowlist unchanged (304 Not Modified)");
            return Ok(None);
        }

        let response = response.error_for_status()?;

        if let Some(etag) = response.headers().get("ETag") {
            *self.etag.lock().await = Some(etag.to_str()?.to_string());
        }

        let list = response.json::<WhitelistResponse>().await?;
        Ok(Some(list.addresses.into_iter().collect()))
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
                        Ok(Some(new_list)) => {
                            let writer = self.allowlist.writer();
                            let mut lock = writer.write().await;

                            if *lock != new_list {
                                *lock = new_list;
                                tracing::debug!("Allowlist updated. {} entries loaded.", lock.len());
                            } else {
                                tracing::debug!("Allowlist unchanged (same content).");
                            }
                        }
                        Ok(None) => {
                            // ETag said "not modified"
                        }
                        Err(err) => {
                            tracing::warn!("Failed to refresh allowlist: {}", err);
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
