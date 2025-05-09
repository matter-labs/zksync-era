//! Dependency injection for gateway contract data.

pub use self::{
    gateway_migrator_layer::GatewayMigratorLayer,
    settlement_layer_data::{ENConfig, MainNodeConfig, SettlementLayerData},
};

mod gateway_migrator_layer;
mod settlement_layer_data;
