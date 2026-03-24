use zksync_config::{configs::wallets, GasAdjusterConfig};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_operator_signer::OperatorSigner;
use zksync_shared_resources::contracts::{
    L1ChainContractsResource, SettlementLayerContractsResource,
};
use zksync_web3_decl::{
    client::{DynClient, L1},
    node::SettlementLayerClient,
};

use super::resources::{BoundEthInterfaceForBlobsResource, BoundEthInterfaceForL2Resource};
use crate::{clients::SigningClient, BoundEthInterface, EthInterface};

/// Wiring layer for creating signing Ethereum clients.
///
/// Supports both local private key and GCP KMS signing.
#[derive(Debug)]
pub struct PKSigningEthClientLayer {
    gas_adjuster_config: GasAdjusterConfig,
    operator: wallets::Wallet,
    blob_operator: Option<wallets::Wallet>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    eth_client: Box<DynClient<L1>>,
    gateway_client: SettlementLayerClient,
    contracts: SettlementLayerContractsResource,
    l1_contracts: L1ChainContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    signing_client: Box<dyn BoundEthInterface>,
    /// Only provided if the blob operator key is provided to the layer.
    signing_client_for_blobs: Option<BoundEthInterfaceForBlobsResource>,
    signing_client_for_gateway: Option<BoundEthInterfaceForL2Resource>,
}

impl PKSigningEthClientLayer {
    pub fn new(
        gas_adjuster_config: GasAdjusterConfig,
        operator: wallets::Wallet,
        blob_operator: Option<wallets::Wallet>,
    ) -> Self {
        Self {
            gas_adjuster_config,
            operator,
            blob_operator,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for PKSigningEthClientLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "pk_signing_eth_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let gas_adjuster_config = &self.gas_adjuster_config;
        let query_client = input.eth_client;

        let l1_diamond_proxy_addr = input
            .l1_contracts
            .0
            .chain_contracts_config
            .diamond_proxy_addr;
        let l1_chain_id = query_client
            .fetch_chain_id()
            .await
            .map_err(WiringError::internal)?;

        let operator_signer = OperatorSigner::from_wallet(&self.operator);
        let operator_address = operator_signer
            .address()
            .await
            .map_err(|e| WiringError::Internal(e.into()))?;
        tracing::info!("Operator address: {operator_address:?}");

        let signing_client = SigningClient::new(
            query_client.clone(),
            zksync_contracts::hyperchain_contract(),
            operator_address,
            operator_signer.clone(),
            l1_diamond_proxy_addr,
            gas_adjuster_config.default_priority_fee_per_gas.into(),
            l1_chain_id,
        );
        let signing_client = Box::new(signing_client);

        let signing_client_for_blobs = match self.blob_operator {
            Some(ref blob_operator) => {
                let blob_signer = OperatorSigner::from_wallet(blob_operator);
                let blob_address = blob_signer
                    .address()
                    .await
                    .map_err(|e| WiringError::Internal(e.into()))?;
                tracing::info!("Blob operator address: {blob_address:?}");

                let signing_client_for_blobs = SigningClient::new(
                    query_client.clone(),
                    zksync_contracts::hyperchain_contract(),
                    blob_address,
                    blob_signer,
                    l1_diamond_proxy_addr,
                    gas_adjuster_config.default_priority_fee_per_gas.into(),
                    l1_chain_id,
                );
                Some(BoundEthInterfaceForBlobsResource(Box::new(
                    signing_client_for_blobs,
                )))
            }
            None => None,
        };

        let signing_client_for_gateway = match input.gateway_client {
            SettlementLayerClient::Gateway(gateway_client) => {
                let l2_chain_id = gateway_client
                    .fetch_chain_id()
                    .await
                    .map_err(WiringError::internal)?;
                let signing_client_for_gateway = SigningClient::new(
                    gateway_client,
                    zksync_contracts::hyperchain_contract(),
                    operator_address,
                    operator_signer,
                    input.contracts.0.chain_contracts_config.diamond_proxy_addr,
                    gas_adjuster_config.default_priority_fee_per_gas.into(),
                    l2_chain_id,
                );
                Some(BoundEthInterfaceForL2Resource(Box::new(
                    signing_client_for_gateway,
                )))
            }
            SettlementLayerClient::L1(_) => None,
        };

        Ok(Output {
            signing_client,
            signing_client_for_blobs,
            signing_client_for_gateway,
        })
    }
}
