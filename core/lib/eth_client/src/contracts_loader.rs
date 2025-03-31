use zksync_config::configs::contracts::{
    chain::ChainContracts, ecosystem::EcosystemCommonContracts, SettlementLayerSpecificContracts,
};
use zksync_contracts::{
    bridgehub_contract, getters_facet_contract, hyperchain_contract,
    state_transition_manager_contract,
};
use zksync_types::{
    ethabi::{Contract, Token},
    protocol_version::ProtocolSemanticVersion,
    settlement::SettlementLayer,
    Address, L2ChainId, SLChainId, U256,
};

use crate::{CallFunctionArgs, ContractCallError, EthInterface};

pub async fn get_diamond_proxy_contract(
    sl_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
) -> anyhow::Result<Address> {
    Ok(
        CallFunctionArgs::new("getHyperchain", Token::Uint(l2_chain_id.as_u64().into()))
            .for_contract(bridgehub_address, &bridgehub_contract())
            .call(sl_client)
            .await?,
    )
}

/// Load contacts specific for each settlement layer, using bridgehub contract
pub async fn load_settlement_layer_contracts(
    sl_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
    multicall3: Option<Address>,
) -> anyhow::Result<Option<SettlementLayerSpecificContracts>> {
    let diamond_proxy =
        get_diamond_proxy_contract(sl_client, bridgehub_address, l2_chain_id).await?;

    if diamond_proxy.is_zero() {
        return Ok(None);
    }

    if !get_protocol_version(diamond_proxy, &hyperchain_contract(), sl_client)
        .await?
        .minor
        .is_post_fflonk()
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
pub async fn get_settlement_layer_from_l1(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<SettlementLayer> {
    let address = get_settlement_layer_address(eth_client, diamond_proxy_addr, abi).await?;
    // Due to implementation details if the settlement layer set to zero,
    // that means the current layer is settlement layer
    let settlement_mode = if address.is_zero() {
        let chain_id = eth_client.fetch_chain_id().await?;
        SettlementLayer::L1(chain_id)
    } else {
        let chain_id = get_diamond_proxy_chain_id(eth_client, address).await?;
        SettlementLayer::Gateway(chain_id)
    };

    Ok(settlement_mode)
}

pub async fn get_settlement_layer_address(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<Address> {
    if !get_protocol_version(diamond_proxy_addr, abi, eth_client)
        .await?
        .minor
        .is_post_fflonk()
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
        .for_contract(diamond_proxy_addr, abi)
        .call(eth_client)
        .await?;

    let protocol_version = ProtocolSemanticVersion::try_from_packed(packed_protocol_version)
        .map_err(|err| anyhow::format_err!("Failed to unpack semver: {err}"))?;
    Ok(protocol_version)
}
