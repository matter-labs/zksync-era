use k8s_openapi::api;
use kube::api::{Api, Patch, PatchParams};

#[derive(Clone)]
pub struct Scaler {
    pub client: kube::Client,
}

impl Scaler {
    pub async fn scale(&self, namespace: &str, name: &str, size: i32) -> anyhow::Result<()> {
        let deployments: Api<api::apps::v1::Deployment> =
            Api::namespaced(self.client.clone(), namespace);

        let patch = serde_json::json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "spec": {
                "replicas": size
            }
        });
        let pp = PatchParams::default();
        let _patched = deployments.patch(name, &pp, &Patch::Merge(patch)).await?;
        tracing::info!("Scaled deployment/{} to {} replica(s).", name, size);

        Ok(())
    }
}
