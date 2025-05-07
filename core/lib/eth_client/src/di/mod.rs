//! Dependency injection for Ethereum client.

pub use self::{
    pk_signing_eth_client::PKSigningEthClientLayer,
    resources::{
        BaseGatewayContractsResource, BaseL1ContractsResource,
        BaseSettlementLayerContractsResource, BoundEthInterfaceForBlobsResource,
        BoundEthInterfaceForL2Resource, BoundEthInterfaceResource, L1EcosystemContractsResource,
        SenderConfigResource,
    },
};

mod pk_signing_eth_client;
mod resources;
