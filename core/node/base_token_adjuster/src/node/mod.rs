//! Dependency injection for base token adjuster.

pub use self::{
    base_token_ratio_persister::BaseTokenRatioPersisterLayer,
    base_token_ratio_provider::BaseTokenRatioProviderLayer,
    price_api_client::ExternalPriceApiLayer, resources::PriceAPIClientResource,
};

mod base_token_ratio_persister;
mod base_token_ratio_provider;
mod price_api_client;
mod resources;
