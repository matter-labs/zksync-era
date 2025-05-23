use zksync_config::{configs::wallets, GasAdjusterConfig};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::contracts::{
    L1ChainContractsResource, SettlementLayerContractsResource,
};
use zksync_web3_decl::node::{EthInterfaceResource, SettlementLayerClient};

use super::resources::{
    BoundEthInterfaceForBlobsResource, BoundEthInterfaceForL2Resource,
    BoundEthInterfaceForTeeDcapResource, BoundEthInterfaceResource,
};
use crate::{clients::PKSigningClient, EthInterface};

/// Wiring layer for [`PKSigningClient`].
#[derive(Debug)]
pub struct PKSigningEthClientLayer {
    gas_adjuster_config: GasAdjusterConfig,
    operator: wallets::Wallet,
    blob_operator: Option<wallets::Wallet>,
    tee_dcap_operator: Option<wallets::Wallet>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
    pub gateway_client: SettlementLayerClient,
    pub contracts: SettlementLayerContractsResource,
    pub l1_contracts: L1ChainContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    pub signing_client: BoundEthInterfaceResource,
    /// Only provided if the blob operator key is provided to the layer.
    pub signing_client_for_blobs: Option<BoundEthInterfaceForBlobsResource>,
    pub signing_client_for_gateway: Option<BoundEthInterfaceForL2Resource>,
    pub signing_client_for_tee_dcap: Option<BoundEthInterfaceForTeeDcapResource>,
}

impl PKSigningEthClientLayer {
    pub fn new(
        gas_adjuster_config: GasAdjusterConfig,
        operator: wallets::Wallet,
        blob_operator: Option<wallets::Wallet>,
        tee_dcap_operator: Option<wallets::Wallet>,
    ) -> Self {
        Self {
            gas_adjuster_config,
            operator,
            blob_operator,
            tee_dcap_operator,
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
        let private_key = self.operator.private_key();
        let gas_adjuster_config = &self.gas_adjuster_config;
        let EthInterfaceResource(query_client) = input.eth_client;

        let l1_diamond_proxy_addr = input
            .l1_contracts
            .0
            .chain_contracts_config
            .diamond_proxy_addr;
        let l1_chain_id = query_client
            .fetch_chain_id()
            .await
            .map_err(WiringError::internal)?;

        let signing_client = PKSigningClient::new_raw(
            private_key.clone(),
            l1_diamond_proxy_addr,
            gas_adjuster_config.default_priority_fee_per_gas,
            l1_chain_id,
            query_client.clone(),
        );
        let signing_client = BoundEthInterfaceResource(Box::new(signing_client));

        let signing_client_for_blobs = self.blob_operator.map(|blob_operator| {
            let private_key = blob_operator.private_key();
            let signing_client_for_blobs = PKSigningClient::new_raw(
                private_key.clone(),
                l1_diamond_proxy_addr,
                gas_adjuster_config.default_priority_fee_per_gas,
                l1_chain_id,
                query_client.clone(),
            );
            BoundEthInterfaceForBlobsResource(Box::new(signing_client_for_blobs))
        });

        // FIXME: TEE
        let signing_client_for_tee_dcap =
            self.tee_dcap_operator.map(|tee_dcap_attestation_operator| {
                let private_key = tee_dcap_attestation_operator.private_key();
                let signing_client_for_tee_dcap = PKSigningClient::new_raw(
                    private_key.clone(),
                    input
                        .l1_contracts
                        .0
                        .ecosystem_contracts
                        .tee_dcap_attestation_addr
                        .unwrap(),
                    gas_adjuster_config.default_priority_fee_per_gas,
                    l1_chain_id,
                    // FIXME: TEE
                    query_client,
                );
                BoundEthInterfaceForTeeDcapResource(Box::new(signing_client_for_tee_dcap))
            });

        let signing_client_for_gateway = match input.gateway_client {
            SettlementLayerClient::L2(gateway_client) => {
                let private_key = self.operator.private_key();
                let l2_chain_id = gateway_client
                    .fetch_chain_id()
                    .await
                    .map_err(WiringError::internal)?;
                let signing_client_for_blobs = PKSigningClient::new_raw(
                    private_key.clone(),
                    input.contracts.0.chain_contracts_config.diamond_proxy_addr,
                    gas_adjuster_config.default_priority_fee_per_gas,
                    l2_chain_id,
                    gateway_client,
                );
                Some(BoundEthInterfaceForL2Resource(Box::new(
                    signing_client_for_blobs,
                )))
            }
            SettlementLayerClient::L1(_) => None,
        };

        Ok(Output {
            signing_client,
            signing_client_for_blobs,
            signing_client_for_gateway,
            signing_client_for_tee_dcap,
        })
    }
}
