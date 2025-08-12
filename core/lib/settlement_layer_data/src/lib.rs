pub mod gateway_urls;
mod node;

pub mod remote_en_config;

use anyhow::Context;
pub use node::{ENConfig, MainNodeConfig, SettlementLayerData};
use zksync_basic_types::{
    ethabi::Contract,
    pubdata_da::PubdataSendingMode,
    settlement::{SettlementLayer, WorkingSettlementLayer},
    url::SensitiveUrl,
    Address, L2ChainId, SLChainId,
};
use zksync_config::configs::{
    contracts::SettlementLayerSpecificContracts, eth_sender::SenderConfig,
};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_eth_client::{
    contracts_loader::{
        get_diamond_proxy_contract, get_settlement_layer_address, get_settlement_layer_from_l1,
        is_settlement_layer,
    },
    ContractCallError, EthInterface,
};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_web3_decl::client::{Client, DynClient, L2};

#[derive(Debug, thiserror::Error)]
pub enum SettlementLayerError {
    #[error("ContractCall Error: {0}")]
    ContractCall(#[from] ContractCallError),
    #[error("Error: {0}")]
    Internal(#[from] anyhow::Error),
}

async fn get_l2_client(
    eth_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
    gateway_rpc_url: Option<SensitiveUrl>,
) -> anyhow::Result<Option<Box<DynClient<L2>>>> {
    // If the server is the settlement layer, the gateway is the server itself,
    // so we can't point to ourselves.
    let is_settlement_layer = is_settlement_layer(eth_client, bridgehub_address, l2_chain_id)
        .await
        .context("failed to call whitelistedSettlementLayers on the bridgehub contract")?;

    if !is_settlement_layer {
        get_l2_client_unchecked(gateway_rpc_url, bridgehub_address).await
    } else {
        Ok(None)
    }
}

async fn get_l2_client_unchecked(
    gateway_rpc_url: Option<SensitiveUrl>,
    l1_bridgehub_address: Address,
) -> anyhow::Result<Option<Box<DynClient<L2>>>> {
    // If gateway rpc is not presented try to fallback to the default gateway url
    let gateway_rpc_url = if let Some(url) = gateway_rpc_url {
        Some(url)
    } else {
        gateway_urls::DefaultGatewayUrl::from_bridgehub_address(l1_bridgehub_address)
            .map(|a| a.to_gateway_url())
    };
    Ok(if let Some(url) = gateway_rpc_url {
        let client: Client<L2> = Client::http(url.clone()).context("Client::new()")?.build();
        let chain_id = client.fetch_chain_id().await?;
        let client = Client::http(url)
            .context("Client::new()")?
            .for_network(L2ChainId::new(chain_id.0).unwrap().into())
            .build();
        Some(Box::new(client))
    } else {
        tracing::warn!(
            "No client was found for gateway, you are working in none \
            ZkSync ecosystem and haven't specified secret for gateway. During the migration it could cause a downtime"
        );
        None
    })
}

// Gateway has different rules for pubdata and gas space.
// We need to adjust it accordingly.
fn adjust_eth_sender_config(
    mut config: SenderConfig,
    settlement_layer: SettlementLayer,
) -> SenderConfig {
    if settlement_layer.is_gateway() {
        config.max_aggregated_tx_gas = 30000000000;
        tracing::warn!(
            "Settling to Gateway requires to adjust ETH sender configs: \
               max_aggregated_tx_gas = {}",
            config.max_aggregated_tx_gas
        );
        if config.pubdata_sending_mode == PubdataSendingMode::Blobs
            || config.pubdata_sending_mode == PubdataSendingMode::Calldata
        {
            tracing::warn!(
                "Settling to Gateway requires to adjust Pub Data Sending Mode: \
                    changed from {:?} to {:?} ",
                &config.pubdata_sending_mode,
                PubdataSendingMode::RelayedL2Calldata
            );
            config.pubdata_sending_mode = PubdataSendingMode::RelayedL2Calldata;
        }
    }
    config
}

// Get settlement layer based on ETH tx in the database. We start on SL matching oldest unfinalized eth tx.
// This due to BatchTransactionUpdater needing this SL to finalize that batch transaction.
async fn get_db_settlement_mode(
    connection: &mut Connection<'_, Core>,
    l1chain_id: SLChainId,
) -> anyhow::Result<Option<SettlementLayer>> {
    let db_chain_id = connection
        .eth_sender_dal()
        .get_chain_id_of_oldest_unfinalized_eth_tx()
        .await?;

    Ok(db_chain_id.map(|chain_id| {
        if chain_id != l1chain_id.0 {
            SettlementLayer::Gateway(SLChainId(chain_id))
        } else {
            SettlementLayer::L1(SLChainId(chain_id))
        }
    }))
}

// Return current settlement layer.
pub async fn current_settlement_layer(
    l1_client: &dyn EthInterface,
    gateway_client: Option<&dyn EthInterface>,
    sl_l1_contracts: &SettlementLayerSpecificContracts,
    l2_chain_id: L2ChainId,
    abi: &Contract,
) -> Result<WorkingSettlementLayer, SettlementLayerError> {
    let settlement_mode_from_l1 = get_settlement_layer_from_l1(
        l1_client,
        sl_l1_contracts.chain_contracts_config.diamond_proxy_addr,
        abi,
    )
    .await?;

    let (sl_client, bridge_hub_address) = match settlement_mode_from_l1 {
        SettlementLayer::L1(_) => (
            l1_client,
            sl_l1_contracts
                .ecosystem_contracts
                .bridgehub_proxy_addr
                .expect("Bridgehub address should always be presented"),
        ),
        SettlementLayer::Gateway(_) => (
            gateway_client.expect("No gateway url was provided"),
            L2_BRIDGEHUB_ADDRESS,
        ),
    };

    // Load chain contracts from sl
    let diamond_proxy_addr =
        get_diamond_proxy_contract(sl_client, bridge_hub_address, l2_chain_id).await?;
    // Deploying contracts on gateway are going through l1->l2 communication,
    // even though the settlement layer has changed on l1.
    // Gateway should process l1->l2 transaction.
    // Even though when we switched from gateway to l1,
    // we don't need to wait for contracts deployment,
    // we have to wait for l2->l1 communication to be finalized
    let use_settlement_mode_from_l1 = if !diamond_proxy_addr.is_zero() {
        let settlement_layer_address =
            get_settlement_layer_address(sl_client, diamond_proxy_addr, abi).await?;

        // When we settle to the current chain, settlement mode should zero
        settlement_layer_address.is_zero()
    } else {
        match settlement_mode_from_l1 {
            // if we want to settle to l1, but no contracts deployed, that means it's pre gateway upgrade and we need to settle to l1
            SettlementLayer::L1(_) => true,
            // if we want to settle to gateway, but no contracts deployed, that means the migration has not been completed yet. We need to continue settle to L1
            SettlementLayer::Gateway(_) => false,
        }
    };

    let final_settlement_mode = if use_settlement_mode_from_l1 {
        settlement_mode_from_l1
    } else {
        // If it's impossible to use settlement_mode_from_l1 server have to use the opposite settlement_layer
        match settlement_mode_from_l1 {
            SettlementLayer::L1(_) => {
                let chain_id = gateway_client
                    .expect("No gateway url was provided")
                    .fetch_chain_id()
                    .await
                    .map_err(ContractCallError::from)?;
                SettlementLayer::Gateway(chain_id)
            }
            SettlementLayer::Gateway(_) => {
                let chain_id = l1_client
                    .fetch_chain_id()
                    .await
                    .map_err(ContractCallError::from)?;
                SettlementLayer::L1(chain_id)
            }
        }
    };

    let mut layer = WorkingSettlementLayer::new(final_settlement_mode);
    layer.set_migration_in_progress(!use_settlement_mode_from_l1);
    Ok(layer)
}
