//! High-level tests for EN.

use assert_matches::assert_matches;
use test_casing::test_casing;
use zksync_basic_types::protocol_version::ProtocolVersionId;
use zksync_core::genesis::{insert_genesis_batch, GenesisParams};
use zksync_eth_client::clients::MockEthereum;
use zksync_types::{api, ethabi, fee_model::FeeParams, L1BatchNumber, L2BlockNumber, H256};
use zksync_web3_decl::client::{BoxedL2Client, MockL2Client};

use super::*;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn block_details_base(hash: H256) -> api::BlockDetailsBase {
    api::BlockDetailsBase {
        timestamp: 0,
        l1_tx_count: 0,
        l2_tx_count: 0,
        root_hash: Some(hash),
        status: api::BlockStatus::Sealed,
        commit_tx_hash: None,
        committed_at: None,
        prove_tx_hash: None,
        proven_at: None,
        execute_tx_hash: None,
        executed_at: None,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        base_system_contracts_hashes: Default::default(),
    }
}

#[derive(Debug)]
struct TestEnvironment {
    sigint_receiver: Option<oneshot::Receiver<()>>,
    app_health_sender: Option<oneshot::Sender<Arc<AppHealthCheck>>>,
}

impl TestEnvironment {
    fn new() -> (Self, TestEnvironmentHandles) {
        let (sigint_sender, sigint_receiver) = oneshot::channel();
        let (app_health_sender, app_health_receiver) = oneshot::channel();
        let this = Self {
            sigint_receiver: Some(sigint_receiver),
            app_health_sender: Some(app_health_sender),
        };
        let handles = TestEnvironmentHandles {
            sigint_sender,
            app_health_receiver,
        };
        (this, handles)
    }
}

impl NodeEnvironment for TestEnvironment {
    fn setup_sigint_handler(&mut self) -> oneshot::Receiver<()> {
        self.sigint_receiver
            .take()
            .expect("requested to setup sigint handler twice")
    }

    fn set_app_health(&mut self, health: Arc<AppHealthCheck>) {
        self.app_health_sender
            .take()
            .expect("set app health twice")
            .send(health)
            .ok();
    }
}

#[derive(Debug)]
struct TestEnvironmentHandles {
    sigint_sender: oneshot::Sender<()>,
    app_health_receiver: oneshot::Receiver<Arc<AppHealthCheck>>,
}

// The returned components have the fully implemented health check life cycle (i.e., signal their shutdown).
fn expected_health_components(components: &ComponentsToRun) -> Vec<&'static str> {
    let mut output = vec!["reorg_detector"];
    if components.0.contains(&Component::Core) {
        output.extend(["consistency_checker", "commitment_generator"]);
    }
    if components.0.contains(&Component::Tree) {
        output.push("tree");
    }
    if components.0.contains(&Component::HttpApi) {
        output.push("http_api");
    }
    if components.0.contains(&Component::WsApi) {
        output.push("ws_api");
    }
    output
}

#[test_casing(5, ["all", "core", "api", "tree", "tree,tree_api"])]
#[tokio::test]
#[tracing::instrument] // Add args to the test logs
async fn external_node_basics(components_str: &'static str) {
    let _guard = vlog::ObservabilityBuilder::new().build(); // Enable logging to simplify debugging
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Simplest case to mock: the EN already has a genesis L1 batch / L2 block, and it's the only L1 batch / L2 block
    // in the network.
    let connection_pool = ConnectionPool::test_pool().await;
    let singleton_pool_builder = ConnectionPool::singleton(connection_pool.database_url());
    let mut storage = connection_pool.connection().await.unwrap();
    let genesis_params = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    let genesis_l2_block = storage
        .blocks_dal()
        .get_l2_block_header(L2BlockNumber(0))
        .await
        .unwrap()
        .expect("No genesis L2 block");
    drop(storage);

    let components: ComponentsToRun = components_str.parse().unwrap();
    let expected_health_components = expected_health_components(&components);
    let opt = Cli {
        revert_pending_l1_batch: false,
        enable_consensus: false,
        components,
    };
    let mut config = ExternalNodeConfig::mock(&temp_dir, &connection_pool);
    if opt.components.0.contains(&Component::TreeApi) {
        config.tree_component.api_port = Some(0);
    }

    let diamond_proxy_addr = config.remote.diamond_proxy_addr;

    let l2_client = MockL2Client::new(move |method, params| {
        tracing::info!("Called L2 client: {method}({params:?})");
        match method {
            "zks_L1BatchNumber" => Ok(serde_json::json!("0x0")),
            "zks_getL1BatchDetails" => {
                let (number,): (L1BatchNumber,) = serde_json::from_value(params)?;
                assert_eq!(number, L1BatchNumber(0));
                Ok(serde_json::to_value(api::L1BatchDetails {
                    number: L1BatchNumber(0),
                    base: block_details_base(genesis_params.root_hash),
                })?)
            }
            "eth_blockNumber" => Ok(serde_json::json!("0x0")),
            "eth_getBlockByNumber" => {
                let (number, _): (api::BlockNumber, bool) = serde_json::from_value(params)?;
                assert_eq!(number, api::BlockNumber::Number(0.into()));
                Ok(serde_json::to_value(
                    api::Block::<api::TransactionVariant> {
                        hash: genesis_l2_block.hash,
                        ..api::Block::default()
                    },
                )?)
            }

            "zks_getFeeParams" => Ok(serde_json::to_value(FeeParams::sensible_v1_default())?),
            "en_whitelistedTokensForAA" => Ok(serde_json::json!([])),

            _ => panic!("Unexpected call: {method}({params:?})"),
        }
    });
    let l2_client = BoxedL2Client::new(l2_client);

    let eth_client = MockEthereum::default().with_call_handler(move |call| {
        tracing::info!("L1 call: {call:?}");
        if call.contract_address() == diamond_proxy_addr {
            match call.function_name() {
                "getPubdataPricingMode" => return ethabi::Token::Uint(0.into()), // "rollup" mode encoding
                "getProtocolVersion" => {
                    return ethabi::Token::Uint((ProtocolVersionId::latest() as u16).into())
                }
                _ => { /* unknown call; panic below */ }
            }
        }
        panic!("Unexpected L1 call: {call:?}");
    });
    let eth_client = Arc::new(eth_client);

    let (env, env_handles) = TestEnvironment::new();
    let node_handle = tokio::spawn(async move {
        run_node(
            env,
            &opt,
            &config,
            connection_pool,
            singleton_pool_builder,
            l2_client,
            eth_client,
        )
        .await
    });

    // Wait until the node is ready.
    let app_health = match env_handles.app_health_receiver.await {
        Ok(app_health) => app_health,
        Err(_) if node_handle.is_finished() => {
            node_handle.await.unwrap().unwrap();
            unreachable!("Node tasks should have panicked or errored");
        }
        Err(_) => unreachable!("Node tasks should have panicked or errored"),
    };

    loop {
        let health_data = app_health.check_health().await;
        tracing::info!(?health_data, "received health data");
        if matches!(health_data.inner().status(), HealthStatus::Ready)
            && expected_health_components
                .iter()
                .all(|name| health_data.components().contains_key(name))
        {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    // Stop the node and check that it timely terminates.
    env_handles.sigint_sender.send(()).unwrap();

    tokio::time::timeout(SHUTDOWN_TIMEOUT, node_handle)
        .await
        .expect("Node hanged up during shutdown")
        .expect("Node panicked")
        .expect("Node errored");

    // Check that the node health was appropriately updated.
    let health_data = app_health.check_health().await;
    tracing::info!(?health_data, "final health data");
    assert_matches!(health_data.inner().status(), HealthStatus::ShutDown);
    for name in expected_health_components {
        let component_health = &health_data.components()[name];
        assert_matches!(component_health.status(), HealthStatus::ShutDown);
    }
}
