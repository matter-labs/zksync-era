//! High-level tests for EN.

use std::time::Duration;

use assert_matches::assert_matches;
use framework::inject_test_layers;
use test_casing::test_casing;
use zksync_health_check::HealthStatus;
use zksync_types::{fee_model::FeeParams, L1BatchNumber, U64};
use zksync_web3_decl::jsonrpsee::core::ClientError;

use super::*;

mod framework;
mod utils;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[test_casing(5, ["all", "core", "api", "core,tree_api", "contract_verification_api"])]
#[tokio::test]
#[tracing::instrument] // Add args to the test logs
async fn external_node_basics(components_str: &'static str) {
    let _guard = zksync_vlog::ObservabilityBuilder::new().try_build().ok(); // Enable logging to simplify debugging
    let (env, env_handles) = utils::TestEnvironment::with_genesis_block(components_str).await;

    let mut expected_health_components = utils::expected_health_components(&env.components);
    let expected_shutdown_components = expected_health_components.clone();
    let has_core_or_api = env.components.0.iter().any(|component| {
        [Component::Core, Component::HttpApi, Component::WsApi].contains(component)
    });
    if has_core_or_api {
        // The `sync_state` component doesn't signal its shutdown, but should be present in the list of components
        expected_health_components.push("sync_state");
    }

    let l2_client = utils::mock_l2_client(&env);
    let eth_client = utils::mock_eth_client(
        env.config.l1_diamond_proxy_address(),
        env.config.remote.l1_bridgehub_proxy_addr.unwrap(),
    );

    let node_handle = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || {
            let mut node = ExternalNodeBuilder::new(env.config)?;
            inject_test_layers(
                &mut node,
                env.sigint_receiver,
                env.app_health_sender,
                eth_client,
                l2_client,
            );

            let node = node.build(env.components.0.into_iter().collect())?;
            node.run(())?;
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
    for name in expected_shutdown_components {
        let component_health = &health_data.components()[name];
        assert_matches!(component_health.status(), HealthStatus::ShutDown);
    }
}

#[tokio::test]
async fn node_reacts_to_stop_signal_during_initial_reorg_detection() {
    let _guard = zksync_vlog::ObservabilityBuilder::new().try_build().ok(); // Enable logging to simplify debugging
    let (env, env_handles) = utils::TestEnvironment::with_genesis_block("core").await;

    let l2_client = utils::mock_l2_client_hanging();
    let eth_client = utils::mock_eth_client(
        env.config.l1_diamond_proxy_address(),
        env.config.remote.l1_bridgehub_proxy_addr.unwrap(),
    );

    let mut node_handle = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || {
            let mut node = ExternalNodeBuilder::new(env.config)?;
            inject_test_layers(
                &mut node,
                env.sigint_receiver,
                env.app_health_sender,
                eth_client,
                l2_client,
            );

            let node = node.build(env.components.0.into_iter().collect())?;
            node.run(())?;
            anyhow::Ok(())
        })
        .join()
        .unwrap()
    });

    // Check that the node doesn't stop on its own.
    let timeout_result = tokio::time::timeout(Duration::from_millis(50), &mut node_handle).await;
    assert_matches!(timeout_result, Err(tokio::time::error::Elapsed { .. }));

    // Send a stop request and check that the node reacts to it.
    env_handles.sigint_sender.send(()).unwrap();
    node_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn running_tree_without_core_is_not_allowed() {
    let _guard = zksync_vlog::ObservabilityBuilder::new().try_build().ok(); // Enable logging to simplify debugging
    let (env, _env_handles) = utils::TestEnvironment::with_genesis_block("tree").await;

    let l2_client = utils::mock_l2_client(&env);
    let eth_client = utils::mock_eth_client(
        env.config.l1_diamond_proxy_address(),
        env.config.remote.l1_bridgehub_proxy_addr.unwrap(),
    );

    let node_handle = tokio::task::spawn_blocking(move || {
        std::thread::spawn(move || {
            let mut node = ExternalNodeBuilder::new(env.config)?;
            inject_test_layers(
                &mut node,
                env.sigint_receiver,
                env.app_health_sender,
                eth_client,
                l2_client,
            );

            // We're only interested in the error, so we drop the result.
            node.build(env.components.0.into_iter().collect()).map(drop)
        })
        .join()
        .unwrap()
    });

    // Check that we cannot build the node without the core component.
    let result = node_handle.await.expect("Building the node panicked");
    let err = result.expect_err("Building the node with tree but without core should fail");
    assert!(
        err.to_string()
            .contains("Tree must run on the same machine as Core"),
        "Unexpected errror: {}",
        err
    );
}
