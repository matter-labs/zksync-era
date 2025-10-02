use k8s_openapi::api;
use kube::api::{Api, Patch, PatchParams};

use crate::cluster_types::{DeploymentName, NamespaceName};

#[derive(Clone)]
pub struct Scaler {
    pub client: kube::Client,
    dry_run: bool,
}

impl Scaler {
    pub fn new(client: kube::Client, dry_run: bool) -> Self {
        Self { client, dry_run }
    }

    pub async fn scale(
        &self,
        namespace: &NamespaceName,
        name: &DeploymentName,
        size: usize,
    ) -> anyhow::Result<()> {
        let deployments: Api<api::apps::v1::Deployment> =
            Api::namespaced(self.client.clone(), namespace.to_str());

        let patch = serde_json::json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "spec": {
                "replicas": size
            }
        });

        if self.dry_run {
            tracing::info!(
                "Dry run of scaled deployment/{} to {} replica(s).",
                name,
                size
            );
            return Ok(());
        }

        let pp = PatchParams::default();
        deployments
            .patch(name.to_str(), &pp, &Patch::Merge(patch))
            .await?;
        tracing::info!("Scaled deployment/{} to {} replica(s).", name, size);

        Ok(())
    }
}
