use std::sync::Arc;

use tokio::sync::oneshot;
use zksync_health_check::{node::AppHealthCheckResource, AppHealthCheck};
use zksync_node_framework::{
    service::ServiceContext, task::TaskKind, FromContext, IntoContext, StopReceiver, Task, TaskId,
    WiringError, WiringLayer,
};
use zksync_types::{L1ChainId, L2ChainId};
use zksync_vlog::node::SigintHandlerLayer;
use zksync_web3_decl::{
    client::{DynClient, MockClient, L1, L2},
    node::{
        MainNodeClientLayer, MainNodeClientResource, QueryEthClientLayer, SettlementLayerClient,
        SettlementLayerClientLayer,
    },
};

use super::ExternalNodeBuilder;

pub(super) fn inject_test_layers(
    node: &mut ExternalNodeBuilder,
    sigint_receiver: oneshot::Receiver<()>,
    app_health_sender: oneshot::Sender<Arc<AppHealthCheck>>,
    l1_client: MockClient<L1>,
    l2_client: MockClient<L2>,
) {
    node.node
        .add_layer(TestSigintLayer {
            receiver: sigint_receiver,
        })
        .add_layer(AppHealthHijackLayer {
            sender: app_health_sender,
        })
        .add_layer(MockL1ClientLayer {
            client: l1_client.clone(),
        })
        .add_layer(MockL2ClientLayer { client: l2_client })
        .add_layer(MockSettlementLayerClientLayer { client: l1_client });
}

/// A test layer that would stop the node upon request.
/// Replaces the `SigintHandlerLayer` in tests.
#[derive(Debug)]
struct TestSigintLayer {
    receiver: oneshot::Receiver<()>,
}

#[async_trait::async_trait]
impl WiringLayer for TestSigintLayer {
    type Input = ();
    type Output = TestSigintTask;

    fn layer_name(&self) -> &'static str {
        // We want to override layer by inserting it first.
        SigintHandlerLayer.layer_name()
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        Ok(TestSigintTask(self.receiver))
    }
}

struct TestSigintTask(oneshot::Receiver<()>);

#[async_trait::async_trait]
impl Task for TestSigintTask {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "test_sigint_task".into()
    }

    async fn run(self: Box<Self>, _: StopReceiver) -> anyhow::Result<()> {
        self.0.await?;
        Ok(())
    }
}

impl IntoContext for TestSigintTask {
    fn into_context(self, context: &mut ServiceContext<'_>) -> Result<(), WiringError> {
        context.add_task(self);
        Ok(())
    }
}

/// Hijacks the `AppHealthCheck` from the context and passes it to the test.
/// Note: It's a separate layer to get access to the app health check, not an override.
#[derive(Debug)]
struct AppHealthHijackLayer {
    sender: oneshot::Sender<Arc<AppHealthCheck>>,
}

#[derive(Debug, FromContext)]
struct AppHealthHijackInput {
    #[context(default)]
    app_health_check: AppHealthCheckResource,
}

#[async_trait::async_trait]
impl WiringLayer for AppHealthHijackLayer {
    type Input = AppHealthHijackInput;
    type Output = ();

    fn layer_name(&self) -> &'static str {
        "app_health_hijack"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        self.sender.send(input.app_health_check.0).unwrap();
        Ok(())
    }
}

#[derive(Debug)]
struct MockL1ClientLayer {
    client: MockClient<L1>,
}

#[async_trait::async_trait]
impl WiringLayer for MockL1ClientLayer {
    type Input = ();
    type Output = Box<DynClient<L1>>;

    fn layer_name(&self) -> &'static str {
        // We don't care about values, we just want to hijack the layer name.
        QueryEthClientLayer::new(L1ChainId(1), "https://example.com".parse().unwrap()).layer_name()
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        Ok(Box::new(self.client))
    }
}

#[derive(Debug)]
struct MockL2ClientLayer {
    client: MockClient<L2>,
}

#[async_trait::async_trait]
impl WiringLayer for MockL2ClientLayer {
    type Input = ();
    type Output = MainNodeClientResource;

    fn layer_name(&self) -> &'static str {
        // We don't care about values, we just want to hijack the layer name.
        MainNodeClientLayer::new(
            "https://example.com".parse().unwrap(),
            100.try_into().unwrap(),
            L2ChainId::default(),
        )
        .layer_name()
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        Ok(MainNodeClientResource(Box::new(self.client)))
    }
}

#[derive(Debug)]
struct MockSettlementLayerClientLayer {
    client: MockClient<L1>,
}

#[async_trait::async_trait]
impl WiringLayer for MockSettlementLayerClientLayer {
    type Input = ();
    type Output = SettlementLayerClient;

    fn layer_name(&self) -> &'static str {
        // We don't care about values, we just want to hijack the layer name.
        SettlementLayerClientLayer::new("https://example.com".parse().unwrap(), None).layer_name()
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        Ok(SettlementLayerClient::L1(Box::new(self.client)))
    }
}
