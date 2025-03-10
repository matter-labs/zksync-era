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

pub async fn load_sl_contracts(
    sl_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
    multicall3: Option<Address>,
) -> anyhow::Result<Option<ChainSpecificContracts>> {
    let gateway_diamond_proxy: Address =
        CallFunctionArgs::new("getZKChain", Token::Uint(l2_chain_id.as_u64().into()))
            .for_contract(bridgehub_address, &bridgehub_contract())
            .call(sl_client)
            .await?;

    if gateway_diamond_proxy.is_zero() {
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
        .await
        .map(|a: Address| if a.is_zero() { None } else { Some(a) })?;

    let chain_admin =
        CallFunctionArgs::new("getChainAdmin", Token::Uint(l2_chain_id.as_u64().into()))
            .for_contract(ctm_address, &state_transition_manager_contract())
            .call(sl_client)
            .await
            .map(|a: Address| if a.is_zero() { None } else { Some(a) })?;

    Ok(Some(ChainSpecificContracts {
        ecosystem_contracts: EcosystemCommonContracts {
            bridgehub_proxy_addr: Some(bridgehub_address),
            state_transition_proxy_addr: Some(ctm_address),
            server_notifier_addr,
            validator_timelock_addr,
            multicall3,
        },
        chain_contracts_config: ChainContracts {
            diamond_proxy_addr: gateway_diamond_proxy,
            chain_admin,
        },
    }))
}

pub async fn load_l1_specific_contracts(
    eth_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
) -> anyhow::Result<L1SpecificContracts> {
    let base_token = CallFunctionArgs::new("baseToken", Token::Uint(l2_chain_id.as_u64().into()))
        .for_contract(bridgehub_address, &bridgehub_contract())
        .call(eth_client)
        .await
        .map(|a: Address| if a.is_zero() { None } else { Some(a) })?;

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
        bytecodes_supplier_addr: None,
        wrapped_base_token_store: None,
    })
}

pub async fn get_settlement_layer(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<SettlementMode> {
    let settlement_layer: Address = CallFunctionArgs::new("getSettlementLayer", ())
        .for_contract(diamond_proxy_addr, abi)
        .call(eth_client)
        .await?;

    let mode = if settlement_layer.is_zero() {
        SettlementMode::SettlesToL1
    } else {
        SettlementMode::Gateway
    };

    Ok(mode)
}
