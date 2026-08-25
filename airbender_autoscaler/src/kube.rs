//! Kubernetes observation and actuation.
//!
//! No agents: the controller talks to each cluster's API server directly via
//! kubeconfig contexts. Deployments must pre-exist (helm/manifests, possibly
//! at 0 replicas); the autoscaler only reads them and patches their scale
//! subresource. GPU card selection is a fixed nodeSelector in the pod
//! template — with a single card class there is no pool dimension here.

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::config::{Config as KubeConfig, KubeConfigOptions, Kubeconfig};
use kube::Client;
use tracing::debug;

use crate::planner::ClusterDeployment;

struct NamedCluster {
    name: String,
    client: Client,
}

pub struct ClusterSet {
    clusters: Vec<NamedCluster>,
    namespace: String,
}

impl ClusterSet {
    /// Connects to the given kubeconfig contexts in fill-priority order; with
    /// no contexts configured, uses the default (current) context.
    pub async fn connect(contexts: &[String], namespace: &str) -> Result<Self> {
        let mut clusters = Vec::new();
        if contexts.is_empty() {
            let client = Client::try_default()
                .await
                .context("connecting with default kubeconfig/in-cluster config")?;
            clusters.push(NamedCluster {
                name: "default".into(),
                client,
            });
        } else {
            let kubeconfig = Kubeconfig::read().context("reading kubeconfig")?;
            for context in contexts {
                let options = KubeConfigOptions {
                    context: Some(context.clone()),
                    ..Default::default()
                };
                let config = KubeConfig::from_custom_kubeconfig(kubeconfig.clone(), &options)
                    .await
                    .with_context(|| format!("loading kubeconfig context `{context}`"))?;
                let client = Client::try_from(config)
                    .with_context(|| format!("building client for context `{context}`"))?;
                clusters.push(NamedCluster {
                    name: context.clone(),
                    client,
                });
            }
        }
        Ok(Self {
            clusters,
            namespace: namespace.to_string(),
        })
    }

    /// Observes one deployment across every cluster, preserving fill-priority
    /// order. Clusters where the deployment doesn't exist are skipped: the
    /// autoscaler never creates deployments, so it cannot schedule there.
    pub async fn observe(&self, deployment_name: &str) -> Result<Vec<ClusterDeployment>> {
        let mut observations = Vec::new();
        for cluster in &self.clusters {
            let deployments: Api<Deployment> =
                Api::namespaced(cluster.client.clone(), &self.namespace);
            let Some(deployment) =
                deployments
                    .get_opt(deployment_name)
                    .await
                    .with_context(|| {
                        format!(
                            "fetching deployment `{deployment_name}` in cluster `{}`",
                            cluster.name
                        )
                    })?
            else {
                debug!(
                    cluster = %cluster.name,
                    deployment = %deployment_name,
                    "deployment not present; skipping cluster"
                );
                continue;
            };

            let spec_replicas = deployment
                .spec
                .as_ref()
                .and_then(|spec| spec.replicas)
                .unwrap_or(0)
                .max(0) as u32;

            let selector = deployment
                .spec
                .as_ref()
                .and_then(|spec| spec.selector.match_labels.as_ref())
                .map(|labels| {
                    labels
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();

            let pods: Api<Pod> = Api::namespaced(cluster.client.clone(), &self.namespace);
            let pod_list = pods
                .list(&ListParams::default().labels(&selector))
                .await
                .with_context(|| {
                    format!(
                        "listing pods for `{deployment_name}` in cluster `{}`",
                        cluster.name
                    )
                })?;

            let now = Utc::now();
            let (mut running, mut pending, mut terminating) = (0u32, 0u32, 0u32);
            let mut oldest_pending_age: Option<Duration> = None;
            for pod in &pod_list.items {
                if pod.metadata.deletion_timestamp.is_some() {
                    terminating += 1;
                    continue;
                }
                match pod
                    .status
                    .as_ref()
                    .and_then(|status| status.phase.as_deref())
                {
                    Some("Running") => running += 1,
                    Some("Succeeded") | Some("Failed") => {}
                    // "Pending", "Unknown", or missing: treat as an
                    // outstanding machine request.
                    _ => {
                        pending += 1;
                        if let Some(created) = pod.metadata.creation_timestamp.as_ref() {
                            let age = (now - created.0).to_std().unwrap_or_default();
                            oldest_pending_age = Some(match oldest_pending_age {
                                Some(existing) => existing.max(age),
                                None => age,
                            });
                        }
                    }
                }
            }

            observations.push(ClusterDeployment {
                cluster: cluster.name.clone(),
                spec_replicas,
                running,
                pending,
                oldest_pending_age,
                terminating,
            });
        }
        Ok(observations)
    }

    pub async fn scale(&self, cluster: &str, deployment_name: &str, replicas: u32) -> Result<()> {
        let target = self
            .clusters
            .iter()
            .find(|c| c.name == cluster)
            .with_context(|| format!("unknown cluster `{cluster}`"))?;
        let deployments: Api<Deployment> = Api::namespaced(target.client.clone(), &self.namespace);
        let patch = serde_json::json!({ "spec": { "replicas": replicas } });
        deployments
            .patch_scale(
                deployment_name,
                &PatchParams::default(),
                &Patch::Merge(&patch),
            )
            .await
            .with_context(|| {
                format!("scaling `{deployment_name}` in cluster `{cluster}` to {replicas}")
            })?;
        Ok(())
    }
}
