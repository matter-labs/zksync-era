use std::{any::Any, fmt};

pub mod object_store;
pub mod pools;
pub mod stop_receiver;

/// A marker trait for anything that can be stored (and retrieved) as a resource.
/// Requires `Clone` since the same resource may be requested by several tasks.
pub trait Resource: 'static + Clone + std::any::Any {}

/// An entitity that knows how to initialize resources.
///
/// It exists to simplify the initialization process, as both tasks and *resources* can depend on other resources,
/// and by having an entity that can initialize the resource on demand we can avoid the need to provide resources
/// in any particular order.
///
/// Node will only call `get_resource` method once per resource, and will cache the result. This guarantees that
/// all the resource consumers will interact with the same resource instance, which may be important for getting
/// the consistent state (e.g. to make sure that L1 gas price is the same for all the tasks).
pub trait ResourceProvider: 'static + Send + Sync + fmt::Debug {
    /// Returns a resource with the given name.
    ///
    /// In case it isn't possible to obtain the resource (for example, if some error occurred during initialization),
    /// the provider is free to either return `None` (if it assumes that the node can continue without this resource),
    /// or to panic.
    fn get_resource(&self, name: &str) -> Option<Box<dyn Any>>;
}
