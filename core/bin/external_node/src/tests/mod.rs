//! High-level tests for EN.

use assert_matches::assert_matches;
use framework::inject_test_layers;
use test_casing::test_casing;
use zksync_types::{api, fee_model::FeeParams, Address, L1BatchNumber, U64};
use zksync_web3_decl::{client::MockClient, jsonrpsee::core::ClientError};

use super::*;

mod framework;
mod utils;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[test_casing(3, ["all", "core", "api"])]
#[tokio::test]
#[tracing::instrument] // Add args to the test logs
async fn external_node_basics(components_str: &'static str) {
    let _guard = zksync_vlog::ObservabilityBuilder::new().build(); // Enable logging to simplify debugging

    let (env, env_handles) = utils::TestEnvironment::with_genesis_block(components_str).await;

    let expected_health_components = utils::expected_health_components(&env.components);
    let diamond_proxy_addr = env.config.remote.diamond_proxy_addr;

    let l2_client = MockClient::builder(L2::default())
        .method("eth_chainId", || Ok(U64::from(270)))
        .method("zks_L1ChainId", || Ok(U64::from(9)))
        .method("zks_L1BatchNumber", || Ok(U64::from(0)))
        .method("zks_getL1BatchDetails", move |number: L1BatchNumber| {
            assert_eq!(number, L1BatchNumber(0));
            Ok(api::L1BatchDetails {
                number: L1BatchNumber(0),
                base: utils::block_details_base(env.genesis_params.root_hash),
            })
        })
        .method("eth_blockNumber", || Ok(U64::from(0)))
        .method(
            "eth_getBlockByNumber",
            move |number: api::BlockNumber, _with_txs: bool| {
                assert_eq!(number, api::BlockNumber::Number(0.into()));
                Ok(api::Block::<api::TransactionVariant> {
                    hash: env.genesis_l2_block.hash,
                    ..api::Block::default()
                })
            },
        )
        .method("zks_getFeeParams", || Ok(FeeParams::sensible_v1_default()))
        .method("en_whitelistedTokensForAA", || Ok([] as [Address; 0]))
        .build();
    // let l2_client = Box::new(l2_client);
    let eth_client = utils::mock_eth_client(diamond_proxy_addr);

    let node_handle = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || {
            let mut node = ExternalNodeBuilder::new(env.config);
            inject_test_layers(
                &mut node,
                env.sigint_receiver,
                env.app_health_sender,
                eth_client,
                l2_client,
            );

            let node = node.build(env.components.0.into_iter().collect())?;
            node.run()?;
            anyhow::Ok(())
        })
        .join()
        .unwrap()
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
    let _guard = zksync_vlog::ObservabilityBuilder::new().build(); // Enable logging to simplify debugging
    let (env, env_handles) = utils::TestEnvironment::with_genesis_block("core").await;

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
    let diamond_proxy_addr = env.config.remote.diamond_proxy_addr;
    let eth_client = utils::mock_eth_client(diamond_proxy_addr);

    let mut node_handle = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || {
            let mut node = ExternalNodeBuilder::new(env.config);
            inject_test_layers(
                &mut node,
                env.sigint_receiver,
                env.app_health_sender,
                eth_client,
                l2_client,
            );

            let node = node.build(env.components.0.into_iter().collect())?;
            node.run()?;
            anyhow::Ok(())
        })
        .join()
        .unwrap()
    });

    // Check that the node doesn't stop on its own.
    let timeout_result = tokio::time::timeout(Duration::from_millis(50), &mut node_handle).await;
    assert_matches!(timeout_result, Err(tokio::time::error::Elapsed { .. }));

    // Send a stop signal and check that the node reacts to it.
    env_handles.sigint_sender.send(()).unwrap();
    node_handle.await.unwrap().unwrap();
}
