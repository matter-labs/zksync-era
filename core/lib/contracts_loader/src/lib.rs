use zksync_config::configs::contracts::{
    chain::ChainContracts, ecosystem::EcosystemCommonContracts, SettlementLayerSpecificContracts,
};
use zksync_contracts::{
    bridgehub_contract, getters_facet_contract, hyperchain_contract,
    state_transition_manager_contract,
};
use zksync_eth_client::{CallFunctionArgs, ContractCallError, EthInterface};
use zksync_types::{
    ethabi::{Contract, Token},
    protocol_version::ProtocolSemanticVersion,
    settlement::SettlementMode,
    Address, L2ChainId, SLChainId, U256,
};

/// Load contacts specific for each settlement layer, using bridgehub contract
pub async fn load_settlement_layer_contracts(
    sl_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
    multicall3: Option<Address>,
) -> anyhow::Result<Option<SettlementLayerSpecificContracts>> {
    let result = load_settlement_layer_contracts_pure_error(
        sl_client,
        bridgehub_address,
        l2_chain_id,
        multicall3,
    )
    .await?;
    Ok(result)
}

async fn load_settlement_layer_contracts_pure_error(
    sl_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
    multicall3: Option<Address>,
) -> anyhow::Result<Option<SettlementLayerSpecificContracts>> {
    let diamond_proxy: Address =
        CallFunctionArgs::new("getHyperchain", Token::Uint(l2_chain_id.as_u64().into()))
            .for_contract(bridgehub_address, &bridgehub_contract())
            .call(sl_client)
            .await?;

    if diamond_proxy.is_zero() {
        return Ok(None);
    }

    if !get_protocol_version(diamond_proxy, &hyperchain_contract(), sl_client)
        .await?
        .minor
        .is_post_gateway()
    {
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

    Ok(Some(SettlementLayerSpecificContracts {
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

/// This function will return correct settlement mode only if it's called for L1.
/// Due to implementation details if the settlement layer set to zero,
/// that means the current layer is settlement layer
pub async fn get_settlement_layer_from_l1(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<(SettlementMode, SLChainId)> {
    let address = get_settlement_layer_address(eth_client, diamond_proxy_addr, abi).await?;
    let (settlement_mode, address) = if address.is_zero() {
        (SettlementMode::SettlesToL1, None)
    } else {
        (SettlementMode::Gateway, Some(address))
    };

    let chain_id = if let Some(address) = address {
        get_diamond_proxy_chain_id(eth_client, address).await?
    } else {
        eth_client.fetch_chain_id().await?
    };

    Ok((settlement_mode, chain_id))
}

pub async fn get_settlement_layer_address(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<Address> {
    if !get_protocol_version(diamond_proxy_addr, abi, eth_client)
        .await?
        .minor
        .is_post_gateway()
    {
        return Ok(Address::zero());
    }
    let settlement_layer: Address = CallFunctionArgs::new("getSettlementLayer", ())
        .for_contract(diamond_proxy_addr, abi)
        .call(eth_client)
        .await?;

    Ok(settlement_layer)
}

async fn get_diamond_proxy_chain_id(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
) -> Result<SLChainId, ContractCallError> {
    let abi = getters_facet_contract();
    let chain_id: U256 = CallFunctionArgs::new("getChainId", ())
        .for_contract(diamond_proxy_addr, &abi)
        .call(eth_client)
        .await?;

    Ok(SLChainId(chain_id.as_u64()))
}

async fn get_protocol_version(
    diamond_proxy_addr: Address,
    abi: &Contract,
    eth_client: &dyn EthInterface,
) -> anyhow::Result<ProtocolSemanticVersion> {
    let packed_protocol_version: U256 = CallFunctionArgs::new("getProtocolVersion", ())
        .for_contract(diamond_proxy_addr, &abi)
        .call(eth_client)
        .await?;

    let protocol_version = ProtocolSemanticVersion::try_from_packed(packed_protocol_version)
        .map_err(|err| anyhow::format_err!("Failed to unpack semver: {err}"))?;
    Ok(protocol_version)
}
