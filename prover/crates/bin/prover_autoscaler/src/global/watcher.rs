use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context, Ok, Result};
use futures::future;
use reqwest::{
    header::{HeaderMap, HeaderValue, CONTENT_TYPE},
    Method,
};
use tokio::sync::Mutex;
use url::Url;
use zksync_prover_task::Task;

use crate::{
    agent::{ScaleRequest, ScaleResponse},
    cluster_types::{Cluster, ClusterName, Clusters},
    http_client::HttpClient,
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

#[derive(Default, Clone)]
pub struct Watcher {
    http_client: HttpClient,
    /// List of base URLs of all agents.
    pub cluster_agents: Vec<Arc<Url>>,
    pub dry_run: bool,
    pub data: Arc<Mutex<WatchedData>>,
}

impl Watcher {
    pub fn new(http_client: HttpClient, agent_urls: Vec<String>, dry_run: bool) -> Self {
        let size = agent_urls.len();
        Self {
            http_client,
            cluster_agents: agent_urls
                .into_iter()
                .map(|u| {
                    Arc::new(
                        Url::parse(&u)
                            .unwrap_or_else(|e| panic!("Unparsable Agent URL {}: {}", u, e)),
                    )
                })
                .collect(),
            dry_run,
            data: Arc::new(Mutex::new(WatchedData {
                clusters: Clusters::default(),
                is_ready: vec![false; size],
            })),
        }
    }

    pub async fn send_scale(
        &self,
        requests: HashMap<ClusterName, ScaleRequest>,
    ) -> anyhow::Result<()> {
        let id_requests: HashMap<usize, ScaleRequest>;
        {
            // Convert cluster names into ids. Holding the data lock.
            let guard = self.data.lock().await;
            id_requests = requests
                .into_iter()
                .filter_map(|(cluster, scale_request)| {
                    guard.clusters.agent_ids.get(&cluster).map_or_else(
                        || {
                            tracing::error!("Failed to find id for cluster {}", cluster);
                            None
                        },
                        |id| Some((*id, scale_request)),
                    )
                })
                .collect();
        }

        let dry_run = self.dry_run;
        let handles: Vec<_> = id_requests
            .into_iter()
            .map(|(id, sr)| {
                let url: String = self.cluster_agents[id]
                    .clone()
                    .join("/scale")
                    .unwrap()
                    .to_string();
                tracing::debug!("Sending scale request to {}, data: {:?}", url, sr);
                let http_client = self.http_client.clone();
                tokio::spawn(async move {
                    let mut headers = HeaderMap::new();
                    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    if dry_run {
                        tracing::info!("Dry-run mode, not sending the request.");
                        return Ok((id, Ok(ScaleResponse::default())));
                    }
                    let response = http_client
                        .send_request_with_retries(
                            &url,
                            Method::POST,
                            Some(headers),
                            Some(serde_json::to_vec(&sr)?),
                        )
                        .await;
                    let response = response.map_err(|err| {
                        anyhow::anyhow!("Failed fetching cluster from url: {url}: {err:?}")
                    })?;
                    let response = response
                        .json::<ScaleResponse>()
                        .await
                        .context("Failed to read response as json");
                    Ok((id, response))
                })
            })
            .collect();

        future::try_join_all(
            future::join_all(handles)
                .await
                .into_iter()
                .map(|h| async move {
                    let (id, res) = h??;

                    let errors: Vec<_> = res
                        .expect("failed to do request to Agent")
                        .scale_result
                        .iter()
                        .filter_map(|e| {
                            if !e.is_empty() {
                                Some(format!("Agent {} failed to scale: {}", id, e))
                            } else {
                                None
                            }
                        })
                        .collect();

                    if !errors.is_empty() {
                        return Err(anyhow!(errors.join(";")));
                    }
                    Ok(())
                })
                .collect::<Vec<_>>(),
        )
        .await?;

        Ok(())
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
                tracing::debug!("Getting cluster data from agent {}", a);
                let http_client = self.http_client.clone();
                tokio::spawn(async move {
                    let url: String = a
                        .clone()
                        .join("/cluster")
                        .context("Failed to join URL with /cluster")?
                        .to_string();
                    let response = http_client
                        .send_request_with_retries(&url, Method::GET, None, None)
                        .await;

                    let response = response.map_err(|err| {
                        anyhow::anyhow!("Failed fetching cluster from url: {url}: {err:?}")
                    })?;
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
                    let cluster = res?;
                    let mut guard = self.data.lock().await;
                    guard.clusters.agent_ids.insert(cluster.name.clone(), i);
                    guard
                        .clusters
                        .clusters
                        .insert(cluster.name.clone(), cluster);
                    guard.is_ready[i] = true;
                    Ok(())
                })
                .collect::<Vec<_>>(),
        )
        .await?;

        Ok(())
    }
}
