use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context, Ok, Result};
use futures::future;
use reqwest::Method;
use tokio::sync::Mutex;
use url::Url;
use zksync_utils::http_with_retries::send_request_with_retries;

use crate::{
    cluster_types::{Cluster, Clusters},
    task_wiring::Task,
};

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

#[derive(Clone)]
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
                        send_request_with_retries(&url, 5, Method::GET, None, None).await;
                    let res = response
                        .map_err(|err| {
                            anyhow::anyhow!("Failed fetching cluster from url: {url}: {err:?}")
                        })?
                        .json::<Cluster>()
                        .await
                        .context("Failed to read response as json");
                    Ok((i, res))
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
