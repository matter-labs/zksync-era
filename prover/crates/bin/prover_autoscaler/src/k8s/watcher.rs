use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use futures::{stream, StreamExt, TryStreamExt};
use k8s_openapi::api;
use kube::{
    api::{Api, ResourceExt},
    runtime::{watcher, WatchStreamExt},
};
use tokio::sync::Mutex;

use crate::cluster_types::{Cluster, Deployment, Namespace, Pod, ScaleEvent};

#[derive(Clone)]
pub struct Watcher {
    pub client: kube::Client,
    pub cluster: Arc<Mutex<Cluster>>,
}

impl Watcher {
    pub fn new(client: kube::Client, cluster_name: String, namespaces: Vec<String>) -> Self {
        let mut ns = HashMap::new();
        namespaces.into_iter().for_each(|n| {
            ns.insert(n, Namespace::default());
        });

        Self {
            client,
            cluster: Arc::new(Mutex::new(Cluster {
                name: cluster_name,
                namespaces: ns,
            })),
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
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
                Api::namespaced(self.client.clone(), namespace);
            watchers.push(
                watcher(deployments, watcher::Config::default())
                    .default_backoff()
                    .applied_objects()
                    .map_ok(Watched::Deploy)
                    .boxed(),
            );

            let pods: Api<api::core::v1::Pod> = Api::namespaced(self.client.clone(), namespace);
            watchers.push(
                watcher(pods, watcher::Config::default())
                    .default_backoff()
                    .applied_objects()
                    .map_ok(Watched::Pod)
                    .boxed(),
            );

            let events: Api<api::core::v1::Event> = Api::namespaced(self.client.clone(), namespace);
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
                            Some(n) => n.to_string(),
                            None => continue,
                        };
                        let mut cluster = self.cluster.lock().await;
                        let v = cluster.namespaces.get_mut(&namespace).unwrap();
                        let dep = v
                            .deployments
                            .entry(d.name_any())
                            .or_insert(Deployment::default());
                        let nums = d.status.clone().unwrap_or_default();
                        dep.running = nums.available_replicas.unwrap_or_default();
                        dep.desired = nums.replicas.unwrap_or_default();

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
                            Some(n) => n.to_string(),
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

                        if pod.status == "Succeeded" || pod.status == "Failed" {
                            // Cleaning up list of pods.
                            v.pods.remove(&p.name_any());
                        }

                        tracing::info!("Got pod: {}", p.name_any())
                    }
                    Watched::Event(e) => {
                        let namespace: String = match e.namespace() {
                            Some(n) => n,
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
