use zksync_config::configs::contracts::{
    chain::ChainContracts, ecosystem::EcosystemCommonContracts, SettlementLayerSpecificContracts,
};
use zksync_contracts::{
    bridgehub_contract, getters_facet_contract, state_transition_manager_contract,
};
use zksync_eth_client::{CallFunctionArgs, ContractCallError, EthInterface};
use zksync_types::{
    ethabi::{Contract, Token},
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
    let (settlement_mode, address) =
        match get_settlement_layer_address(eth_client, diamond_proxy_addr, abi).await {
            Err(ContractCallError::Function(_)) => {
                // Pre Gateway upgrade contracts, it's safe to say we are settling the data on L1
                (SettlementMode::SettlesToL1, None)
            }
            Err(err) => Err(err)?,
            Ok(address) => {
                if address.is_zero() {
                    (SettlementMode::SettlesToL1, None)
                } else {
                    (SettlementMode::Gateway, Some(address))
                }
            }
        };

    let chain_id = if let Some(address) = address {
        get_settlement_layer_chain_id(eth_client, address).await?
    } else {
        eth_client.fetch_chain_id().await?
    };

    Ok((settlement_mode, chain_id))
}

pub async fn get_settlement_layer_address(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> Result<Address, ContractCallError> {
    let settlement_layer: Address = CallFunctionArgs::new("getSettlementLayer", ())
        .for_contract(diamond_proxy_addr, abi)
        .call(eth_client)
        .await?;

    Ok(settlement_layer)
}

pub async fn get_settlement_layer_chain_id(
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
