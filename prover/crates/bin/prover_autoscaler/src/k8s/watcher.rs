use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use chrono::{DateTime, Utc};
use futures::{stream, StreamExt, TryStreamExt};
use k8s_openapi::api;
use kube::{
    api::{Api, ListParams, ResourceExt},
    runtime::{watcher, WatchStreamExt},
    Resource,
};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Method,
};
use tokio::{sync::Mutex, time::interval};

use crate::{
    cluster_types::{Cluster, ClusterName, Deployment, Namespace, NamespaceName, Pod, ScaleEvent},
    config::ProverAutoscalerAgentConfig,
    http_client::HttpClient,
    metrics::AUTOSCALER_METRICS,
};

#[derive(Clone)]
pub struct Watcher {
    pub client: kube::Client,
    pub cluster: Arc<Mutex<Cluster>>,
    pub config: ProverAutoscalerAgentConfig,
}

async fn get_cluster_name(http_client: HttpClient) -> anyhow::Result<ClusterName> {
    let mut headers = HeaderMap::new();
    headers.insert("Metadata-Flavor", HeaderValue::from_static("Google"));
    let url = "http://metadata.google.internal/computeMetadata/v1/instance/attributes/cluster-name";
    let response = http_client
        .send_request_with_retries(url, Method::GET, Some(headers), None)
        .await;
    Ok(response
        .map_err(|err| anyhow::anyhow!("Failed fetching response from url: {url}: {err:?}"))?
        .text()
        .await
        .context("Failed to read response as text")?
        .into())
}

impl Watcher {
    pub async fn new(
        http_client: HttpClient,
        client: kube::Client,
        config: ProverAutoscalerAgentConfig,
        cluster_name: Option<ClusterName>,
    ) -> Self {
        let mut ns = HashMap::new();
        config.namespaces.iter().for_each(|n| {
            ns.insert(n.clone(), Namespace::default());
        });

        let cluster_name = match cluster_name {
            Some(c) => c,
            None => get_cluster_name(http_client)
                .await
                .expect("Load cluster_name from GCP"),
        };
        tracing::info!("Agent cluster name is {cluster_name}");

        Self {
            client,
            cluster: Arc::new(Mutex::new(Cluster {
                name: cluster_name,
                namespaces: ns,
            })),
            config,
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let cluster_clone_for_cleanup = Arc::clone(&self.cluster);
        let client_clone_for_cleanup = self.client.clone();
        let pod_check_interval = self.config.pod_check_interval;
        tokio::spawn(async move {
            let mut ticker = interval(pod_check_interval);
            loop {
                ticker.tick().await;
                tracing::info!("Running periodic pod cleanup task...");
                let namespaces_to_check: Vec<NamespaceName> = {
                    let cluster_guard = cluster_clone_for_cleanup.lock().await;
                    cluster_guard.namespaces.keys().cloned().collect()
                };

                for ns_name in namespaces_to_check {
                    let pods_api: Api<api::core::v1::Pod> =
                        Api::namespaced(client_clone_for_cleanup.clone(), ns_name.to_str());
                    match pods_api.list(&ListParams::default()).await {
                        Ok(live_pods_list) => {
                            let live_pod_names: std::collections::HashSet<String> =
                                live_pods_list.items.iter().map(|p| p.name_any()).collect();

                            let mut cluster_guard = cluster_clone_for_cleanup.lock().await;
                            if let Some(namespace_data) = cluster_guard.namespaces.get_mut(&ns_name)
                            {
                                let stored_pod_names: Vec<String> =
                                    namespace_data.pods.keys().cloned().collect();
                                for pod_name in stored_pod_names {
                                    if !live_pod_names.contains(&pod_name) {
                                        tracing::warn!(
                                            "Pods cleanup: Removing missing pod {} from namespace {}.",
                                            pod_name,
                                            ns_name
                                        );
                                        AUTOSCALER_METRICS.stale_pods[&pod_name.clone()].inc();
                                        namespace_data.pods.remove(&pod_name);
                                        // TODO: Add more specific monitoring metrics for each missing pod if a metrics system is available
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "Periodic cleanup: Failed to list pods in namespace {}: {:?}",
                                ns_name,
                                e
                            );
                        }
                    }
                }
            }
        });

        // TODO: add actual metrics
        // TODO: watch for a list of namespaces, get:
        //  - deployments (name, running, desired) [done]
        //  - pods (name, parent deployment, statuses, when the last status change) [~done]
        //  - events (number of scheduling failures in last N seconds, which deployments)
        //  - events (preemptions, which deployment, when, how many)
        //  - pool size from GCP (name, size, which GPU)
        let mut watchers = vec![];
        for namespace in self.cluster.lock().await.namespaces.keys() {
            let deployments: Api<api::apps::v1::Deployment> =
                Api::namespaced(self.client.clone(), namespace.to_str());
            watchers.push(
                watcher(deployments, watcher::Config::default())
                    .default_backoff()
                    .applied_objects()
                    .map_ok(Watched::Deploy)
                    .boxed(),
            );

            let pods: Api<api::core::v1::Pod> =
                Api::namespaced(self.client.clone(), namespace.to_str());
            watchers.push(
                watcher(pods, watcher::Config::default())
                    .default_backoff()
                    .applied_objects()
                    .map_ok(Watched::Pod)
                    .boxed(),
            );

            let events: Api<api::core::v1::Event> =
                Api::namespaced(self.client.clone(), namespace.to_str());
            watchers.push(
                watcher(events, watcher::Config::default())
                    .default_backoff()
                    .applied_objects()
                    .map_ok(Watched::Event)
                    .boxed(),
            );
        }
        // select on applied events from all watchers
        let mut combo_stream = stream::select_all(watchers);
        // SelectAll Stream elements must have the same Item, so all packed in this:
        #[allow(clippy::large_enum_variant)]
        enum Watched {
            Deploy(api::apps::v1::Deployment),
            Pod(api::core::v1::Pod),
            Event(api::core::v1::Event),
        }
        while let Some(o) = combo_stream.next().await {
            match o {
                Ok(o) => match o {
                    Watched::Deploy(d) => {
                        let namespace = match d.namespace() {
                            Some(n) => n.into(),
                            None => continue,
                        };
                        let mut cluster = self.cluster.lock().await;
                        let v = cluster.namespaces.get_mut(&namespace).unwrap();
                        let dep = v
                            .deployments
                            .entry(d.name_any().into())
                            .or_insert(Deployment::default());
                        let nums = d.status.clone().unwrap_or_default();
                        dep.running = nums.available_replicas.unwrap_or_default() as usize;
                        dep.desired = nums.replicas.unwrap_or_default() as usize;

                        tracing::info!(
                            "Got deployment: {}, size: {}/{} un {}",
                            d.name_any(),
                            nums.available_replicas.unwrap_or_default(),
                            nums.replicas.unwrap_or_default(),
                            nums.unavailable_replicas.unwrap_or_default(),
                        )
                    }
                    Watched::Pod(p) => {
                        let namespace = match p.namespace() {
                            Some(n) => n.into(),
                            None => continue,
                        };
                        let mut cluster = self.cluster.lock().await;
                        let v = cluster.namespaces.get_mut(&namespace).unwrap();
                        let pod = v.pods.entry(p.name_any()).or_insert(Pod::default());
                        pod.owner = p
                            .owner_references()
                            .iter()
                            .map(|x| format!("{}/{}", x.kind.clone(), x.name.clone()))
                            .collect::<Vec<String>>()
                            .join(":");
                        // TODO: Collect replica sets to match deployments and pods.
                        let phase = p
                            .status
                            .clone()
                            .unwrap_or_default()
                            .phase
                            .unwrap_or_default();
                        if phase != pod.status {
                            // TODO: try to get an idea how to set correct value on restart.
                            pod.changed = Utc::now();
                        }
                        pod.status = phase;

                        if p.meta().deletion_timestamp.is_some()
                            || pod.status == "Succeeded"
                            || pod.status == "Failed"
                        {
                            // Cleaning up list of pods.
                            tracing::debug!("Remove pod: {}", &p.name_any());
                            v.pods.remove(&p.name_any());
                        }

                        tracing::info!("Got pod: {}", p.name_any())
                    }
                    Watched::Event(e) => {
                        let namespace = match e.namespace() {
                            Some(n) => n.into(),
                            None => "".into(),
                        };
                        let name = e.name_any();
                        let reason = e.reason.unwrap_or_default();
                        if reason != "FailedScaleUp" {
                            // Ignore all events which are not scale issues.
                            continue;
                        }
                        let time: DateTime<Utc> = match e.last_timestamp {
                            Some(t) => t.0,
                            None => Utc::now(),
                        };
                        tracing::debug!(
                            "Got event: {}/{}, message: {:?}; action: {:?}, reason: {:?}",
                            namespace,
                            name,
                            e.message.unwrap_or_default(),
                            e.action.unwrap_or_default(),
                            reason
                        );
                        let mut cluster = self.cluster.lock().await;
                        let v = cluster.namespaces.get_mut(&namespace).unwrap();
                        v.scale_errors.push(ScaleEvent { name, time })
                    }
                },
                Err(err) => tracing::warn!("Error during watch: {err:?}"),
            }
        }

        Ok(())
    }
}
