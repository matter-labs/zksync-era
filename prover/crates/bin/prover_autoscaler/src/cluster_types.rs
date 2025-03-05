use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};
use strum::{Display, EnumString};

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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ScaleEvent {
    pub name: String,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Namespace {
    #[serde(serialize_with = "ordered_map")]
    pub deployments: HashMap<String, Deployment>,
    pub pods: HashMap<String, Pod>,
    #[serde(default)]
    pub scale_errors: Vec<ScaleEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cluster {
    pub name: String,
    pub namespaces: HashMap<String, Namespace>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Clusters {
    pub clusters: HashMap<String, Cluster>,
    /// Map from cluster to index in agent URLs Vec.
    pub agent_ids: HashMap<String, usize>,
}

#[derive(Default, Debug, EnumString, Display, Hash, PartialEq, Eq, Clone, Copy)]
pub enum PodStatus {
    #[default]
    Unknown,
    Running,
    Pending,
    LongPending,
    NeedToMove,
    Failed,
}
