use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context, Ok, Result};
use futures::future;
use reqwest::Method;
use tokio::sync::Mutex;
use url::Url;
use zksync_utils::http_with_retries::send_request_with_retries;

use crate::{
    cluster_types::{Cluster, Clusters},
    metrics::{AUTOSCALER_METRICS, DEFAULT_ERROR_CODE},
    task_wiring::Task,
};

const MAX_RETRIES: usize = 5;

#[derive(Default)]
pub struct WatchedData {
    pub clusters: Clusters,
    pub is_ready: Vec<bool>,
}

pub fn check_is_ready(v: &Vec<bool>) -> Result<()> {
    for b in v {
        if !b {
            return Err(anyhow!("Clusters data is not ready"));
        }
    }
    Ok(())
}

#[derive(Default, Clone)]
pub struct Watcher {
    /// List of base URLs of all agents.
    pub cluster_agents: Vec<Arc<Url>>,
    pub data: Arc<Mutex<WatchedData>>,
}

impl Watcher {
    pub fn new(agent_urls: Vec<String>) -> Self {
        let size = agent_urls.len();
        Self {
            cluster_agents: agent_urls
                .into_iter()
                .map(|u| {
                    Arc::new(
                        Url::parse(&u)
                            .unwrap_or_else(|e| panic!("Unparsable Agent URL {}: {}", u, e)),
                    )
                })
                .collect(),
            data: Arc::new(Mutex::new(WatchedData {
                clusters: Clusters {
                    clusters: HashMap::new(),
                },
                is_ready: vec![false; size],
            })),
        }
    }
}

#[async_trait::async_trait]
impl Task for Watcher {
    async fn invoke(&self) -> anyhow::Result<()> {
        let handles: Vec<_> = self
            .cluster_agents
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                tracing::debug!("Getting cluster data from agent {}.", a);
                tokio::spawn(async move {
                    let url: String = a
                        .clone()
                        .join("/cluster")
                        .context("Failed to join URL with /cluster")?
                        .to_string();
                    let response =
                        send_request_with_retries(&url, MAX_RETRIES, Method::GET, None, None).await;

                    let response = response.map_err(|err| {
                        // TODO: refactor send_request_with_retries to return status.
                        AUTOSCALER_METRICS.calls[&(url.clone(), DEFAULT_ERROR_CODE)].inc();
                        anyhow::anyhow!("Failed fetching cluster from url: {url}: {err:?}")
                    })?;
                    AUTOSCALER_METRICS.calls[&(url, response.status().as_u16())].inc();
                    let response = response
                        .json::<Cluster>()
                        .await
                        .context("Failed to read response as json");
                    Ok((i, response))
                })
            })
            .collect();

        future::try_join_all(
            future::join_all(handles)
                .await
                .into_iter()
                .map(|h| async move {
                    let (i, res) = h??;
                    let c = res?;
                    let mut guard = self.data.lock().await;
                    guard.clusters.clusters.insert(c.name.clone(), c);
                    guard.is_ready[i] = true;
                    Ok(())
                })
                .collect::<Vec<_>>(),
        )
        .await?;

        Ok(())
    }
}
