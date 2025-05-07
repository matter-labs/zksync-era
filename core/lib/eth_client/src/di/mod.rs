//! Dependency injection for Ethereum client.

pub use self::{
    main_node_client::MainNodeClientLayer,
    pk_signing_eth_client::PKSigningEthClientLayer,
    query_eth_client::QueryEthClientLayer,
    resources::{
        BaseGatewayContractsResource, BaseL1ContractsResource, BoundEthInterfaceForBlobsResource,
        BoundEthInterfaceForL2Resource, BoundEthInterfaceResource, EthInterfaceResource,
        L2InterfaceResource, MainNodeClientResource, SettlementLayerClient,
    },
    settlement_layer_client::SettlementLayerClientLayer,
};

mod main_node_client;
mod pk_signing_eth_client;
mod query_eth_client;
mod resources;
mod settlement_layer_client;
