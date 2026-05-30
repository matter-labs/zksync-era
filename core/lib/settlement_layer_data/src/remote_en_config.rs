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
            .and_then(|a| a.bridgehub_proxy_addr),
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

#[cfg(test)]
mod tests {
    use zksync_basic_types::{
        commitment::L1BatchCommitmentMode, protocol_version::ProtocolSemanticVersion, Address,
        L1ChainId, L2ChainId, H256,
    };
    use zksync_types::api::BridgeAddresses;
    use zksync_web3_decl::{
        client::{DynClient, MockClient, L2},
        types::{EcosystemContractsDto, GenesisConfigDto},
    };

    use super::fetch_remote_en_config;

    fn mock_main_node_client(ecosystem_contracts: EcosystemContractsDto) -> Box<DynClient<L2>> {
        let client = MockClient::builder(L2::default())
            .method("zks_getBridgeContracts", || {
                Ok(BridgeAddresses {
                    l1_shared_default_bridge: Some(Address::repeat_byte(1)),
                    l2_shared_default_bridge: Some(Address::repeat_byte(3)),
                    l1_erc20_default_bridge: Some(Address::repeat_byte(4)),
                    l2_erc20_default_bridge: Some(Address::repeat_byte(3)),
                    l1_weth_bridge: None,
                    l2_weth_bridge: None,
                    l2_legacy_shared_bridge: Some(Address::repeat_byte(5)),
                })
            })
            .method("zks_getTestnetPaymaster", || Ok(Address::repeat_byte(15)))
            .method("en_getEcosystemContracts", move || {
                Ok(ecosystem_contracts.clone())
            })
            .method("zks_getMainContract", || Ok(Address::repeat_byte(9)))
            .method("zks_getTimestampAsserter", || {
                Ok(Some(Address::repeat_byte(6)))
            })
            .method("zks_getL2Multicall3", || Ok(Some(Address::repeat_byte(7))))
            .method("zks_getBaseTokenL1Address", || Ok(Address::repeat_byte(8)))
            .method("genesis_config", || {
                Ok(GenesisConfigDto {
                    protocol_version: ProtocolSemanticVersion::default(),
                    genesis_root_hash: H256::repeat_byte(1),
                    rollup_last_leaf_index: 0,
                    genesis_commitment: H256::repeat_byte(2),
                    bootloader_hash: H256::repeat_byte(3),
                    default_aa_hash: H256::repeat_byte(4),
                    evm_emulator_hash: None,
                    l1_chain_id: L1ChainId(9),
                    l2_chain_id: L2ChainId::default(),
                    snark_wrapper_vk_hash: H256::repeat_byte(5),
                    fflonk_snark_wrapper_vk_hash: None,
                    fee_account: Address::repeat_byte(10),
                    dummy_verifier: false,
                    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode::default(),
                })
            })
            .build();

        Box::new(client)
    }

    #[tokio::test]
    async fn fetch_remote_en_config_allows_missing_bridgehub_addr() {
        let remote_config = fetch_remote_en_config(mock_main_node_client(EcosystemContractsDto {
            bridgehub_proxy_addr: None,
            state_transition_proxy_addr: Some(Address::repeat_byte(11)),
            message_root_proxy_addr: Some(Address::repeat_byte(12)),
            transparent_proxy_admin_addr: Address::zero(),
            l1_bytecodes_supplier_addr: Some(Address::repeat_byte(13)),
            l1_wrapped_base_token_store: Some(Address::repeat_byte(14)),
            server_notifier_addr: Some(Address::repeat_byte(15)),
        }))
        .await
        .unwrap();

        assert_eq!(remote_config.l1_bridgehub_proxy_addr, None);
        assert_eq!(
            remote_config.l1_state_transition_proxy_addr,
            Some(Address::repeat_byte(11))
        );
    }

    #[tokio::test]
    async fn fetch_remote_en_config_preserves_present_bridgehub_addr() {
        let remote_config = fetch_remote_en_config(mock_main_node_client(EcosystemContractsDto {
            bridgehub_proxy_addr: Some(Address::repeat_byte(21)),
            state_transition_proxy_addr: Some(Address::repeat_byte(11)),
            message_root_proxy_addr: Some(Address::repeat_byte(12)),
            transparent_proxy_admin_addr: Address::zero(),
            l1_bytecodes_supplier_addr: Some(Address::repeat_byte(13)),
            l1_wrapped_base_token_store: Some(Address::repeat_byte(14)),
            server_notifier_addr: Some(Address::repeat_byte(15)),
        }))
        .await
        .unwrap();

        assert_eq!(
            remote_config.l1_bridgehub_proxy_addr,
            Some(Address::repeat_byte(21))
        );
        assert_eq!(
            remote_config.l1_message_root_proxy_addr,
            Some(Address::repeat_byte(12))
        );
    }
}
