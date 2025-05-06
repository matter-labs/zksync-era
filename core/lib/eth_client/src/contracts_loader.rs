use anyhow::Context;
use zksync_config::configs::contracts::{
    chain::ChainContracts, ecosystem::EcosystemCommonContracts, SettlementLayerSpecificContracts,
};
use zksync_contracts::{
    bridgehub_contract, hyperchain_contract, state_transition_manager_contract,
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
) -> Result<Address, ContractCallError> {
    CallFunctionArgs::new("getHyperchain", Token::Uint(l2_chain_id.as_u64().into()))
        .for_contract(bridgehub_address, &bridgehub_contract())
        .call(sl_client)
        .await
}

pub async fn get_server_notifier_addr(
    sl_client: &dyn EthInterface,
    ctm_address: Address,
) -> Result<Option<Address>, ContractCallError> {
    CallFunctionArgs::new("serverNotifierAddress", ())
        .for_contract(ctm_address, &state_transition_manager_contract())
        .call(sl_client)
        .await
        .map(|a: Address| if a.is_zero() { None } else { Some(a) })
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

    if !ProtocolSemanticVersion::try_from_packed(
        get_protocol_version(diamond_proxy, &hyperchain_contract(), sl_client).await?,
    )
    .map_err(|err| anyhow::format_err!("Failed to unpack semver: {err}"))?
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

    let validator_timelock_addr = CallFunctionArgs::new("validatorTimelock", ())
        .for_contract(ctm_address, &state_transition_manager_contract())
        .call(sl_client)
        .await?;

    Ok(Some(SettlementLayerSpecificContracts {
        ecosystem_contracts: EcosystemCommonContracts {
            bridgehub_proxy_addr: Some(bridgehub_address),
            state_transition_proxy_addr: Some(ctm_address),
            validator_timelock_addr: Some(validator_timelock_addr),
            multicall3,
        },
        chain_contracts_config: ChainContracts {
            diamond_proxy_addr: diamond_proxy,
        },
    }))
}

pub async fn get_settlement_layer_from_bridgehub(
    eth_client: &dyn EthInterface,
    l1_chain_id: SLChainId,
    bridgehub_address: Address,
    chain_id: SLChainId,
    bridgehub_abi: &Contract,
) -> Result<Option<SettlementLayer>, ContractCallError> {
    let settlement_layer_chain_id: U256 =
        CallFunctionArgs::new("settlementLayer", U256::from(chain_id.0))
            .for_contract(bridgehub_address, bridgehub_abi)
            .call(eth_client)
            .await?;
    if settlement_layer_chain_id == U256::zero() {
        return Ok(None);
    }

    let settlement_layer_chain_id = SLChainId(settlement_layer_chain_id.as_u64());

    let settlement_mode = if l1_chain_id == settlement_layer_chain_id {
        SettlementLayer::L1(l1_chain_id)
    } else {
        SettlementLayer::Gateway(settlement_layer_chain_id)
    };

    Ok(Some(settlement_mode))
}

pub async fn get_settlement_layer_from_l1_bridgehub(
    eth_client: &dyn EthInterface,
    bridgehub_address: Address,
    chain_id: SLChainId,
    bridgehub_abi: &Contract,
) -> anyhow::Result<SettlementLayer> {
    let l1_chain_id = eth_client.fetch_chain_id().await?;

    let settlement_layer = get_settlement_layer_from_bridgehub(
        eth_client,
        l1_chain_id,
        bridgehub_address,
        chain_id,
        bridgehub_abi,
    )
    .await?;

    settlement_layer.context("Missing settlement layer on L1 bridgehub")
}

async fn get_protocol_version(
    diamond_proxy_addr: Address,
    abi: &Contract,
    eth_client: &dyn EthInterface,
) -> Result<U256, ContractCallError> {
    CallFunctionArgs::new("getProtocolVersion", ())
        .for_contract(diamond_proxy_addr, abi)
        .call(eth_client)
        .await
}
