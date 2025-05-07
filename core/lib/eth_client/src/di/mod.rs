//! Dependency injection for Ethereum client.

pub use self::{
    pk_signing_eth_client::PKSigningEthClientLayer,
    resources::{
        BaseGatewayContractsResource, BaseL1ContractsResource, BoundEthInterfaceForBlobsResource,
        BoundEthInterfaceForL2Resource, BoundEthInterfaceResource, L1EcosystemContractsResource,
        SenderConfigResource, SettlementLayerContractsResource,
    },
};

mod pk_signing_eth_client;
mod resources;
