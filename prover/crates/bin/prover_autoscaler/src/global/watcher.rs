use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context, Ok};
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
    metrics::AUTOSCALER_METRICS,
};

#[derive(Debug, Default, Clone)]
pub struct WatchedData {
    pub clusters: Clusters,
    pub is_ready: Vec<bool>,
}

#[derive(Clone)]
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

    pub fn check_is_ready(&self, watched_data: &WatchedData) -> anyhow::Result<()> {
        let mut ready_count = 0;
        for (id, is_agent_ready) in watched_data.is_ready.iter().enumerate() {
            if *is_agent_ready {
                ready_count += 1;
            } else {
                let agent_url_str = self.cluster_agents[id].to_string();
                AUTOSCALER_METRICS.agent_not_ready[&agent_url_str].inc();
                tracing::warn!("Agent at URL '{}' (id {}) is not ready.", agent_url_str, id);
            }
        }

        if ready_count == 0 {
            return Err(anyhow::anyhow!("All cluster agents are not ready"));
        }
        if ready_count < watched_data.is_ready.len() {
            tracing::warn!(
                "{} out of {} cluster agents are ready. Proceeding with available agents.",
                ready_count,
                watched_data.is_ready.len()
            );
        }
        Ok(())
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

    pub fn create_poller_tasks(&self) -> impl Iterator<Item = AgentPoller> + '_ {
        self.cluster_agents.iter().enumerate().map(|(id, url)| {
            AgentPoller::new(id, url.clone(), self.http_client.clone(), self.data.clone())
        })
    }
}

pub struct AgentPoller {
    agent_id: usize,
    agent_url: Arc<Url>,
    http_client: HttpClient,
    data: Arc<Mutex<WatchedData>>,
}

impl AgentPoller {
    pub fn new(
        agent_id: usize,
        agent_url: Arc<Url>,
        http_client: HttpClient,
        data: Arc<Mutex<WatchedData>>,
    ) -> Self {
        Self {
            agent_id,
            agent_url,
            http_client,
            data,
        }
    }
}

#[async_trait::async_trait]
impl Task for AgentPoller {
    async fn invoke(&self) -> anyhow::Result<()> {
        let url = self
            .agent_url
            .join("/cluster")
            .context("Failed to join URL with /cluster")?
            .to_string();
        let agent_url = self.agent_url.to_string();

        tracing::debug!("AgentPoller: Getting cluster data from agent {}", url);
        match self
            .http_client
            .send_request_with_retries(&url, Method::GET, None, None)
            .await
        {
            Err(http_err) => {
                tracing::error!(
                    "AgentPoller: Failed fetching cluster data from agent {}: {:?}",
                    agent_url,
                    http_err
                );
                AUTOSCALER_METRICS.agent_not_ready[&agent_url].inc();
                let mut guard = self.data.lock().await;
                guard.is_ready[self.agent_id] = false;
            }
            std::result::Result::Ok(response) => match response.json::<Cluster>().await {
                Err(json_err) => {
                    tracing::error!(
                        "AgentPoller: Failed to parse JSON cluster data from agent {}: {:?}",
                        agent_url,
                        json_err
                    );
                    AUTOSCALER_METRICS.agent_not_ready[&agent_url].inc();
                    let mut guard = self.data.lock().await;
                    guard.is_ready[self.agent_id] = false;
                }
                std::result::Result::Ok(cluster) => {
                    let mut guard = self.data.lock().await;
                    guard
                        .clusters
                        .agent_ids
                        .insert(cluster.name.clone(), self.agent_id);
                    guard
                        .clusters
                        .clusters
                        .insert(cluster.name.clone(), cluster);
                    guard.is_ready[self.agent_id] = true;
                }
            },
        }
        Ok(())
    }
}
