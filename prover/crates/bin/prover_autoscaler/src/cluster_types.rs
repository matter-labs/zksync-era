use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    str::FromStr,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};
use strum::{Display, EnumString};
use vise::_reexports::encoding::{EncodeLabelValue, LabelValueEncoder};

#[derive(Debug)]
pub struct ParseAError;
impl fmt::Display for ParseAError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("")
    }
}

string_type!(ClusterName);
string_type!(NamespaceName);
string_type!(DeploymentName);

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pod {
    pub owner: String,
    pub status: String,
    pub changed: DateTime<Utc>,
    pub out_of_resources: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Deployment {
    pub running: usize,
    pub desired: usize,
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
    pub deployments: HashMap<DeploymentName, Deployment>,
    pub pods: HashMap<String, Pod>,
    #[serde(default)]
    pub scale_errors: Vec<ScaleEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cluster {
    pub name: ClusterName,
    pub namespaces: HashMap<NamespaceName, Namespace>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Clusters {
    pub clusters: HashMap<ClusterName, Cluster>,
    /// Map from cluster to index in agent URLs Vec.
    pub agent_ids: HashMap<ClusterName, usize>,
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
