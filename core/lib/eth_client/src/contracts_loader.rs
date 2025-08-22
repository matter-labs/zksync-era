use zksync_config::configs::contracts::{
    chain::{ChainContracts, ZkChainOnChainConfig},
    ecosystem::EcosystemCommonContracts,
    SettlementLayerSpecificContracts,
};
use zksync_contracts::{
    bridgehub_contract, getters_facet_contract, hyperchain_contract,
    state_transition_manager_contract,
};
use zksync_types::{
    commitment::L2DACommitmentScheme,
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
    CallFunctionArgs::new("getZKChain", Token::Uint(l2_chain_id.as_u64().into()))
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

    let protocol_version =
        get_protocol_version(diamond_proxy, &hyperchain_contract(), sl_client).await?;
    let protocol_version = ProtocolSemanticVersion::try_from_packed(protocol_version)
        .map_err(|err| anyhow::format_err!("Failed to unpack semver: {err}"))?;

    if !protocol_version.minor.is_post_fflonk() {
        return Ok(None);
    }

    let ctm_address =
        CallFunctionArgs::new("chainTypeManager", Token::Uint(l2_chain_id.as_u64().into()))
            .for_contract(bridgehub_address, &bridgehub_contract())
            .call(sl_client)
            .await?;

    let message_root_proxy_addr = CallFunctionArgs::new("messageRoot", ())
        .for_contract(bridgehub_address, &bridgehub_contract())
        .call(sl_client)
        .await?;

    let validator_timelock_addr = if protocol_version.minor.is_pre_interop_fast_blocks() {
        CallFunctionArgs::new("validatorTimelock", ())
            .for_contract(ctm_address, &state_transition_manager_contract())
            .call(sl_client)
            .await?
    } else {
        CallFunctionArgs::new("validatorTimelockPostV29", ())
            .for_contract(ctm_address, &state_transition_manager_contract())
            .call(sl_client)
            .await?
    };

    Ok(Some(SettlementLayerSpecificContracts {
        ecosystem_contracts: EcosystemCommonContracts {
            bridgehub_proxy_addr: Some(bridgehub_address),
            state_transition_proxy_addr: Some(ctm_address),
            message_root_proxy_addr: Some(message_root_proxy_addr),
            validator_timelock_addr: Some(validator_timelock_addr),
            multicall3,
        },
        chain_contracts_config: ChainContracts {
            diamond_proxy_addr: diamond_proxy,
        },
    }))
}

/// This function will return correct settlement mode only if it's called for L1.
pub async fn get_settlement_layer_from_l1(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> Result<SettlementLayer, ContractCallError> {
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
) -> Result<Address, ContractCallError> {
    if !ProtocolSemanticVersion::try_from_packed(
        get_protocol_version(diamond_proxy_addr, &hyperchain_contract(), eth_client).await?,
    )
    // It's safe to unwrap it, because we have the same mechanism on l1
    .unwrap()
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
) -> Result<U256, ContractCallError> {
    CallFunctionArgs::new("getProtocolVersion", ())
        .for_contract(diamond_proxy_addr, abi)
        .call(eth_client)
        .await
}

pub async fn is_settlement_layer(
    eth_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
) -> Result<bool, ContractCallError> {
    let is_settlement_layer: bool = CallFunctionArgs::new(
        "whitelistedSettlementLayers",
        Token::Uint(l2_chain_id.as_u64().into()),
    )
    .for_contract(bridgehub_address, &bridgehub_contract())
    .call(eth_client)
    .await?;
    Ok(is_settlement_layer)
}

pub async fn get_zk_chain_on_chain_params(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
) -> Result<ZkChainOnChainConfig, ContractCallError> {
    let abi = getters_facet_contract();
    let l2_da_commitment_scheme: Token = CallFunctionArgs::new("getDAValidatorPair", ())
        .for_contract(diamond_proxy_addr, &abi)
        .call(eth_client)
        .await?;
    dbg!(&l2_da_commitment_scheme);
    let l2_da_commitment_scheme =
        if let Token::Tuple(l2_da_commitment_scheme) = l2_da_commitment_scheme {
            if let [Token::Address(_), Token::Uint(l2_da_commitment_scheme)] =
                l2_da_commitment_scheme.as_slice()
            {
                Some(
                    L2DACommitmentScheme::try_from(l2_da_commitment_scheme.as_u64() as u8)
                        .expect("Invalid L2DACommitmentScheme value"),
                )
            } else {
                None
            }
        } else {
            None
        };

    Ok(ZkChainOnChainConfig {
        l2_da_commitment_scheme,
    })
}
