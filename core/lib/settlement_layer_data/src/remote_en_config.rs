use std::future::Future;

use anyhow::Context;
use zksync_config::RemoteENConfig;
use zksync_eth_client::ClientError;
use zksync_system_constants::ETHEREUM_ADDRESS;
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::ClientRpcContext,
    jsonrpsee::types::ErrorCode,
    namespaces::{EnNamespaceClient, ZksNamespaceClient},
};

pub(crate) async fn fetch_remote_en_config(
    client: Box<DynClient<L2>>,
) -> anyhow::Result<RemoteENConfig> {
    let bridges = client
        .get_bridge_contracts()
        .rpc_context("get_bridge_contracts")
        .await?;

    let l2_testnet_paymaster_addr = client
        .get_testnet_paymaster()
        .rpc_context("get_testnet_paymaster")
        .await?;
    let genesis = client.genesis_config().rpc_context("genesis").await.ok();

    let l1_ecosystem_contracts = client
        .get_ecosystem_contracts()
        .rpc_context("l1_ecosystem_contracts")
        .await
        .ok();
    let l1_diamond_proxy_addr = client
        .get_main_l1_contract()
        .rpc_context("get_main_l1_contract")
        .await?;

    let timestamp_asserter_address = handle_rpc_response_with_fallback(
        client.get_timestamp_asserter(),
        None,
        "Failed to fetch timestamp asserter address".to_string(),
    )
    .await?;
    let l2_multicall3 = handle_rpc_response_with_fallback(
        client.get_l2_multicall3(),
        None,
        "Failed to fetch l2 multicall3".to_string(),
    )
    .await?;
    let base_token_addr = handle_rpc_response_with_fallback(
        client.get_base_token_l1_address(),
        ETHEREUM_ADDRESS,
        "Failed to fetch base token address".to_string(),
    )
    .await?;

    let l2_erc20_default_bridge = bridges
        .l2_erc20_default_bridge
        .or(bridges.l2_shared_default_bridge)
        .unwrap();
    let l2_erc20_shared_bridge = bridges
        .l2_shared_default_bridge
        .or(bridges.l2_erc20_default_bridge)
        .unwrap();

    if l2_erc20_default_bridge != l2_erc20_shared_bridge {
        panic!("L2 erc20 bridge address and L2 shared bridge address are different.");
    }

    Ok(RemoteENConfig {
        l1_bridgehub_proxy_addr: l1_ecosystem_contracts
            .as_ref()
            .map(|a| a.bridgehub_proxy_addr),
        l1_state_transition_proxy_addr: l1_ecosystem_contracts
            .as_ref()
            .and_then(|a| a.state_transition_proxy_addr),
        l1_bytecodes_supplier_addr: l1_ecosystem_contracts
            .as_ref()
            .and_then(|a| a.l1_bytecodes_supplier_addr),
        l1_wrapped_base_token_store: l1_ecosystem_contracts
            .as_ref()
            .and_then(|a| a.l1_wrapped_base_token_store),
        l1_server_notifier_addr: l1_ecosystem_contracts
            .as_ref()
            .and_then(|a| a.server_notifier_addr),
        l1_diamond_proxy_addr,
        l2_testnet_paymaster_addr,
        l1_erc20_bridge_proxy_addr: bridges.l1_erc20_default_bridge,
        l2_erc20_bridge_addr: l2_erc20_default_bridge,
        l1_shared_bridge_proxy_addr: bridges.l1_shared_default_bridge,
        l2_shared_bridge_addr: l2_erc20_shared_bridge,
        l2_legacy_shared_bridge_addr: bridges.l2_legacy_shared_bridge,
        base_token_addr,
        l2_multicall3,
        l1_batch_commit_data_generator_mode: genesis
            .as_ref()
            .map(|a| a.l1_batch_commit_data_generator_mode)
            .unwrap_or_default(),
        dummy_verifier: genesis
            .as_ref()
            .map(|a| a.dummy_verifier)
            .unwrap_or_default(),
        l2_timestamp_asserter_addr: timestamp_asserter_address,
        l1_message_root_proxy_addr: l1_ecosystem_contracts
            .as_ref()
            .and_then(|a| a.message_root_proxy_addr),
    })
}

async fn handle_rpc_response_with_fallback<T, F>(
    rpc_call: F,
    fallback: T,
    context: String,
) -> anyhow::Result<T>
where
    F: Future<Output = Result<T, ClientError>>,
    T: Clone,
{
    match rpc_call.await {
        Err(ClientError::Call(err))
            if [
                ErrorCode::MethodNotFound.code(),
                // This what `Web3Error::NotImplemented` gets
                // `casted` into in the `api` server.
                ErrorCode::InternalError.code(),
            ]
            .contains(&(err.code())) =>
        {
            Ok(fallback)
        }
        response => response.context(context),
    }
}
