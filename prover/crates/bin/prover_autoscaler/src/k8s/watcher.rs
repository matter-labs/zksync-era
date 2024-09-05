use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use futures::{stream, StreamExt, TryStreamExt};
use k8s_openapi::api;
use kube::{
    api::{Api, ResourceExt},
    runtime::{watcher, WatchStreamExt},
};
use serde::{Deserialize, Serialize, Serializer};
use tokio::sync::Mutex;

use crate::metrics::AUTOSCALER_METRICS;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pod {
    // pub name: String, // TODO: Consider if it's needed.
    pub owner: String,
    pub status: String,
    pub changed: DateTime<Utc>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Deployment {
    // pub name: String, // TODO: Consider if it's needed.
    pub running: i32,
    pub desired: i32,
}

fn ordered_map<S, K: Ord + Serialize, V: Serialize>(
    value: &HashMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let ordered: BTreeMap<_, _> = value.iter().collect();
    ordered.serialize(serializer)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    #[serde(serialize_with = "ordered_map")]
    pub deployments: HashMap<String, Deployment>,
    pub pods: HashMap<String, Pod>,
}
impl Namespace {
    pub fn new() -> Self {
        Self {
            deployments: HashMap::new(),
            pods: HashMap::new(),
        }
    }
}
impl Default for Namespace {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub namespaces: HashMap<String, Namespace>,
}

#[derive(Clone)]
pub struct Watcher {
    pub client: kube::Client,
    pub cluster: Arc<Mutex<Cluster>>,
}

impl Watcher {
    pub fn new(client: kube::Client, namespaces: Vec<String>) -> Self {
        let mut ns = HashMap::new();
        namespaces.into_iter().for_each(|n| {
            ns.insert(n, Namespace::default());
        });

        Self {
            client,
            cluster: Arc::new(Mutex::new(Cluster { namespaces: ns })),
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        // TODO: add actual metrics
        AUTOSCALER_METRICS.protocol_version.set(1);
        AUTOSCALER_METRICS.calls.inc_by(1);

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
        }
        // select on applied events from all watchers
        let mut combo_stream = stream::select_all(watchers);
        // SelectAll Stream elements must have the same Item, so all packed in this:
        #[allow(clippy::large_enum_variant)]
        enum Watched {
            Deploy(api::apps::v1::Deployment),
            Pod(api::core::v1::Pod),
        }
        while let Some(o) = combo_stream.try_next().await? {
            match o {
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

                    println!(
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

                    println!("Got pod: {}", p.name_any())
                }
            }
        }

        Ok(())
    }
}
