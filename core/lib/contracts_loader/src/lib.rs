use zksync_config::configs::contracts::{
    chain::ChainContracts,
    ecosystem::{EcosystemCommonContracts, L1SpecificContracts},
    ChainSpecificContracts,
};
use zksync_contracts::{bridgehub_contract, state_transition_manager_contract};
use zksync_eth_client::{CallFunctionArgs, EthInterface};
use zksync_types::{
    ethabi::{Contract, Token},
    settlement::SettlementMode,
    Address, L2ChainId,
};

/// Load contacts specific for each settlement layer, using bridgehub contract
pub async fn load_settlement_layer_contracts(
    sl_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
    multicall3: Option<Address>,
) -> anyhow::Result<Option<ChainSpecificContracts>> {
    let diamond_proxy: Address =
        CallFunctionArgs::new("getZKChain", Token::Uint(l2_chain_id.as_u64().into()))
            .for_contract(bridgehub_address, &bridgehub_contract())
            .call(sl_client)
            .await?;

    if diamond_proxy.is_zero() {
        return Ok(None);
    }

    let ctm_address =
        CallFunctionArgs::new("chainTypeManager", Token::Uint(l2_chain_id.as_u64().into()))
            .for_contract(bridgehub_address, &bridgehub_contract())
            .call(sl_client)
            .await?;

    let server_notifier_addr = CallFunctionArgs::new("serverNotifierAddress", ())
        .for_contract(ctm_address, &state_transition_manager_contract())
        .call(sl_client)
        .await
        .map(|a: Address| if a.is_zero() { None } else { Some(a) })?;

    let validator_timelock_addr = CallFunctionArgs::new("validatorTimelock", ())
        .for_contract(ctm_address, &state_transition_manager_contract())
        .call(sl_client)
        .await?;

    let chain_admin =
        CallFunctionArgs::new("getChainAdmin", Token::Uint(l2_chain_id.as_u64().into()))
            .for_contract(ctm_address, &state_transition_manager_contract())
            .call(sl_client)
            .await?;

    Ok(Some(ChainSpecificContracts {
        ecosystem_contracts: EcosystemCommonContracts {
            bridgehub_proxy_addr: Some(bridgehub_address),
            state_transition_proxy_addr: Some(ctm_address),
            server_notifier_addr,
            validator_timelock_addr: Some(validator_timelock_addr),
            multicall3,
        },
        chain_contracts_config: ChainContracts {
            diamond_proxy_addr: diamond_proxy,
            chain_admin: Some(chain_admin),
        },
    }))
}

// Load Contracts specific for l1, from bridgehub contracts
pub async fn load_l1_specific_contracts(
    eth_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
) -> anyhow::Result<L1SpecificContracts> {
    let base_token = CallFunctionArgs::new("baseToken", Token::Uint(l2_chain_id.as_u64().into()))
        .for_contract(bridgehub_address, &bridgehub_contract())
        .call(eth_client)
        .await?;

    let shared_bridge = CallFunctionArgs::new("sharedBridge", ())
        .for_contract(bridgehub_address, &bridgehub_contract())
        .call(eth_client)
        .await
        .map(|a: Address| if a.is_zero() { None } else { Some(a) })?;

    let erc20_bridge = CallFunctionArgs::new("legacyBridge", ())
        .for_contract(bridgehub_address, &bridgehub_contract())
        .call(eth_client)
        .await
        .map(|a: Address| if a.is_zero() { None } else { Some(a) })?;

    Ok(L1SpecificContracts {
        bridge_hub: Some(bridgehub_address),
        shared_bridge,
        base_token_address: base_token,
        erc_20_bridge: erc20_bridge,
        // TODO add support for loading bytecodes_supplier_addr and
        // wrapped_base_token_store from bridgehub
        bytecodes_supplier_addr: None,
        wrapped_base_token_store: None,
    })
}

pub async fn get_settlement_layer_for_l1_call(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<SettlementMode> {
    let settlement_layer =
        get_settlement_layer_address(eth_client, diamond_proxy_addr, abi).await?;

    let mode = if settlement_layer.is_zero() {
        SettlementMode::SettlesToL1
    } else {
        SettlementMode::Gateway
    };

    Ok(mode)
}

pub async fn get_settlement_layer_address(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<Address> {
    let settlement_layer: Address = CallFunctionArgs::new("getSettlementLayer", ())
        .for_contract(diamond_proxy_addr, abi)
        .call(eth_client)
        .await?;

    Ok(settlement_layer)
}
