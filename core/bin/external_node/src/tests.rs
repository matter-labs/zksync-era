//! High-level tests for EN.

use assert_matches::assert_matches;
use test_casing::test_casing;
use zksync_dal::CoreDal;
use zksync_eth_client::clients::MockEthereum;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_types::{
    api, ethabi, fee_model::FeeParams, Address, L1BatchNumber, L2BlockNumber, ProtocolVersionId,
    H256, U64,
};
use zksync_web3_decl::{
    client::{MockClient, L1},
    jsonrpsee::core::ClientError,
};

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
        fair_pubdata_price: None,
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

fn mock_eth_client(diamond_proxy_addr: Address) -> MockClient<L1> {
    let mock = MockEthereum::builder().with_call_handler(move |call, _| {
        tracing::info!("L1 call: {call:?}");
        if call.to == Some(diamond_proxy_addr) {
            let packed_semver = ProtocolVersionId::latest().into_packed_semver_with_patch(0);
            let call_signature = &call.data.as_ref().unwrap().0[..4];
            let contract = zksync_contracts::hyperchain_contract();
            let pricing_mode_sig = contract
                .function("getPubdataPricingMode")
                .unwrap()
                .short_signature();
            let protocol_version_sig = contract
                .function("getProtocolVersion")
                .unwrap()
                .short_signature();
            match call_signature {
                sig if sig == pricing_mode_sig => {
                    return ethabi::Token::Uint(0.into()); // "rollup" mode encoding
                }
                sig if sig == protocol_version_sig => return ethabi::Token::Uint(packed_semver),
                _ => { /* unknown call; panic below */ }
            }
        }
        panic!("Unexpected L1 call: {call:?}");
    });
    mock.build().into_client()
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
    let singleton_pool_builder = ConnectionPool::singleton(connection_pool.database_url().clone());
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
        enable_consensus: false,
        components,
        use_node_framework: false,
    };
    let mut config = ExternalNodeConfig::mock(&temp_dir, &connection_pool);
    if opt.components.0.contains(&Component::TreeApi) {
        config.tree_component.api_port = Some(0);
    }

    let diamond_proxy_addr = config.remote.diamond_proxy_addr;

    let l2_client = MockClient::builder(L2::default())
        .method("eth_chainId", || Ok(U64::from(270)))
        .method("zks_L1ChainId", || Ok(U64::from(9)))
        .method("zks_L1BatchNumber", || Ok(U64::from(0)))
        .method("zks_getL1BatchDetails", move |number: L1BatchNumber| {
            assert_eq!(number, L1BatchNumber(0));
            Ok(api::L1BatchDetails {
                number: L1BatchNumber(0),
                base: block_details_base(genesis_params.root_hash),
            })
        })
        .method("eth_blockNumber", || Ok(U64::from(0)))
        .method(
            "eth_getBlockByNumber",
            move |number: api::BlockNumber, _with_txs: bool| {
                assert_eq!(number, api::BlockNumber::Number(0.into()));
                Ok(api::Block::<api::TransactionVariant> {
                    hash: genesis_l2_block.hash,
                    ..api::Block::default()
                })
            },
        )
        .method("zks_getFeeParams", || Ok(FeeParams::sensible_v1_default()))
        .method("en_whitelistedTokensForAA", || Ok([] as [Address; 0]))
        .build();
    let l2_client = Box::new(l2_client);
    let eth_client = Box::new(mock_eth_client(diamond_proxy_addr));

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

#[tokio::test]
async fn node_reacts_to_stop_signal_during_initial_reorg_detection() {
    let _guard = vlog::ObservabilityBuilder::new().build(); // Enable logging to simplify debugging
    let temp_dir = tempfile::TempDir::new().unwrap();

    let connection_pool = ConnectionPool::test_pool().await;
    let singleton_pool_builder = ConnectionPool::singleton(connection_pool.database_url().clone());
    let mut storage = connection_pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    drop(storage);

    let opt = Cli {
        enable_consensus: false,
        components: "core".parse().unwrap(),
        use_node_framework: false,
    };
    let mut config = ExternalNodeConfig::mock(&temp_dir, &connection_pool);
    if opt.components.0.contains(&Component::TreeApi) {
        config.tree_component.api_port = Some(0);
    }

    let l2_client = MockClient::builder(L2::default())
        .method("eth_chainId", || Ok(U64::from(270)))
        .method("zks_L1ChainId", || Ok(U64::from(9)))
        .method("zks_L1BatchNumber", || {
            Err::<(), _>(ClientError::RequestTimeout)
        })
        .method("eth_blockNumber", || {
            Err::<(), _>(ClientError::RequestTimeout)
        })
        .method("zks_getFeeParams", || Ok(FeeParams::sensible_v1_default()))
        .method("en_whitelistedTokensForAA", || Ok([] as [Address; 0]))
        .build();
    let l2_client = Box::new(l2_client);
    let diamond_proxy_addr = config.remote.diamond_proxy_addr;
    let eth_client = Box::new(mock_eth_client(diamond_proxy_addr));

    let (env, env_handles) = TestEnvironment::new();
    let mut node_handle = tokio::spawn(async move {
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

    // Check that the node doesn't stop on its own.
    let timeout_result = tokio::time::timeout(Duration::from_millis(50), &mut node_handle).await;
    assert_matches!(timeout_result, Err(tokio::time::error::Elapsed { .. }));

    // Send a stop signal and check that the node reacts to it.
    env_handles.sigint_sender.send(()).unwrap();
    node_handle.await.unwrap().unwrap();
}
