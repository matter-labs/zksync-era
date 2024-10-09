use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Ok};
use futures::future;
use reqwest::Method;
use tokio::sync::Mutex;
use url::Url;
use zksync_utils::http_with_retries::send_request_with_retries;

use crate::{
    cluster_types::{Cluster, Clusters},
    task_wiring::Task,
};

#[derive(Clone)]
pub struct Watcher {
    /// List of base URLs of all agents.
    pub cluster_agents: Vec<Arc<Url>>,
    pub clusters: Arc<Mutex<Clusters>>,
}

impl Watcher {
    pub fn new(agent_urls: Vec<String>) -> Self {
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
            clusters: Arc::new(Mutex::new(Clusters {
                clusters: HashMap::new(),
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
            .map(|a| {
                tracing::debug!("Getting cluster data from agent {}.", a);
                tokio::spawn(async move {
                    let url: String = a
                        .clone()
                        .join("/cluster")
                        .context("Failed to join URL with /cluster")?
                        .to_string();
                    let response =
                        send_request_with_retries(&url, 5, Method::GET, None, None).await;
                    response
                        .map_err(|err| {
                            anyhow::anyhow!("Failed fetching cluster from url: {url}: {err:?}")
                        })?
                        .json::<Cluster>()
                        .await
                        .context("Failed to read response as json")
                })
            })
            .collect();

        future::try_join_all(
            future::join_all(handles)
                .await
                .into_iter()
                .map(|h| async move {
                    let c = h.unwrap().unwrap();
                    self.clusters
                        .lock()
                        .await
                        .clusters
                        .insert(c.name.clone(), c);
                    Ok(())
                })
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap();

        Ok(())
    }
}
