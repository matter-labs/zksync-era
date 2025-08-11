use std::future::Future;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{commitment::L1BatchCommitmentMode, Address};
use zksync_config::configs::contracts::{
    chain::{ChainContracts, L2Contracts},
    ecosystem::{EcosystemCommonContracts, L1SpecificContracts},
    SettlementLayerSpecificContracts,
};
use zksync_eth_client::ClientError;
use zksync_system_constants::ETHEREUM_ADDRESS;
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::ClientRpcContext,
    jsonrpsee::types::ErrorCode,
    namespaces::{EnNamespaceClient, ZksNamespaceClient},
};

/// This part of the external node config is fetched directly from the main node.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RemoteENConfig {
    pub l1_bytecodes_supplier_addr: Option<Address>,
    pub l1_bridgehub_proxy_addr: Option<Address>,
    pub l1_state_transition_proxy_addr: Option<Address>,
    pub l1_diamond_proxy_addr: Address,
    // While on L1 shared bridge and legacy bridge are different contracts with different addresses,
    // the `l2_erc20_bridge_addr` and `l2_shared_bridge_addr` are basically the same contract, but with
    // a different name, with names adapted only for consistency.
    pub l1_shared_bridge_proxy_addr: Option<Address>,
    /// Contract address that serves as a shared bridge on L2.
    /// It is expected that `L2SharedBridge` is used before gateway upgrade, and `L2AssetRouter` is used after.
    pub l2_shared_bridge_addr: Address,
    /// Address of `L2SharedBridge` that was used before gateway upgrade.
    /// `None` if chain genesis used post-gateway protocol version.
    pub l2_legacy_shared_bridge_addr: Option<Address>,
    pub l1_erc20_bridge_proxy_addr: Option<Address>,
    pub l2_erc20_bridge_addr: Address,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub l2_timestamp_asserter_addr: Option<Address>,
    pub l1_wrapped_base_token_store: Option<Address>,
    pub l1_server_notifier_addr: Option<Address>,
    pub l1_message_root_proxy_addr: Option<Address>,
    pub base_token_addr: Address,
    pub l2_multicall3: Option<Address>,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub dummy_verifier: bool,
}

impl RemoteENConfig {
    pub async fn fetch(client: Box<DynClient<L2>>) -> anyhow::Result<Self> {
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

        Ok(Self {
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

    pub fn l1_specific_contracts(&self) -> L1SpecificContracts {
        L1SpecificContracts {
            bytecodes_supplier_addr: self.l1_bytecodes_supplier_addr,
            wrapped_base_token_store: self.l1_wrapped_base_token_store,
            bridge_hub: self.l1_bridgehub_proxy_addr,
            shared_bridge: self.l1_shared_bridge_proxy_addr,
            message_root: self.l1_message_root_proxy_addr,
            erc_20_bridge: self.l1_erc20_bridge_proxy_addr,
            base_token_address: self.base_token_addr,
            server_notifier_addr: self.l1_server_notifier_addr,
            // We don't need chain admin for external node
            chain_admin: None,
        }
    }

    pub fn l1_settelment_contracts(&self) -> SettlementLayerSpecificContracts {
        SettlementLayerSpecificContracts {
            ecosystem_contracts: EcosystemCommonContracts {
                bridgehub_proxy_addr: self.l1_bridgehub_proxy_addr,
                state_transition_proxy_addr: self.l1_state_transition_proxy_addr,
                message_root_proxy_addr: self.l1_message_root_proxy_addr,
                // Multicall 3 is useless for external node
                multicall3: None,
                validator_timelock_addr: None,
            },
            chain_contracts_config: ChainContracts {
                diamond_proxy_addr: self.l1_diamond_proxy_addr,
            },
        }
    }

    pub fn l2_contracts(&self) -> L2Contracts {
        L2Contracts {
            erc20_default_bridge: self.l2_erc20_bridge_addr,
            shared_bridge_addr: self.l2_shared_bridge_addr,
            legacy_shared_bridge_addr: self.l2_legacy_shared_bridge_addr,
            timestamp_asserter_addr: self.l2_timestamp_asserter_addr,
            testnet_paymaster_addr: self.l2_testnet_paymaster_addr,
            multicall3: self.l2_multicall3,
            da_validator_addr: None,
        }
    }
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
