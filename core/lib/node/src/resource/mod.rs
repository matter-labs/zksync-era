use std::{any::Any, fmt};

pub mod object_store;
pub mod pools;
pub mod state_keeper;
pub mod stop_receiver;

/// A marker trait for anything that can be stored (and retrieved) as a resource.
/// Requires `Clone` since the same resource may be requested by several tasks.
///
/// TODO: More elaborate docs.
pub trait Resource: 'static + Clone + std::any::Any {}

pub trait ResourceProvider: 'static + Send + Sync + fmt::Debug {
    /// Resources are expected to be available.
    /// In case it isn't possible to obtain the resource, the provider is free to either return `None`
    /// (if it assumes that the node can continue without this resource), or to panic.
    fn get_resource(&self, name: &str) -> Option<Box<dyn Any>>;
}
