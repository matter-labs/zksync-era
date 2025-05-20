//! Dependency injection for Ethereum client.

// Re-export to allow all `zksync_eth_client` users to not import the `zksync_shared_di` crate directly.
pub use zksync_shared_resources::contracts;

pub use self::{
    bridge_addresses::BridgeAddressesUpdaterLayer,
    pk_signing_eth_client::PKSigningEthClientLayer,
    resources::{
        BoundEthInterfaceForBlobsResource, BoundEthInterfaceForL2Resource,
        BoundEthInterfaceResource, SenderConfigResource,
    },
};

mod bridge_addresses;
mod pk_signing_eth_client;
mod resources;
