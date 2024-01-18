use std::{any::Any, fmt};

pub use self::resource_collection::ResourceCollection;

pub mod resource_collection;

/// A marker trait for anything that can be stored (and retrieved) as a resource.
/// Requires `Clone` since the same resource may be requested by several tasks.
pub trait Resource: 'static + Clone + std::any::Any {
    /// Unique identifier of the resource.
    /// Used to fetch the resource from the provider.
    ///
    /// It is recommended to name resources in form of `<scope>/<name>`, where `<scope>` is the name of the task
    /// that will use this resource, or 'common' in case it is used by several tasks, and `<name>` is the name
    /// of the resource itself.
    const RESOURCE_NAME: &'static str;
}

/// An entity that knows how to initialize resources.
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
    // Note: we have to use `Box<dyn Any>` here, since we can't use `Box<dyn Resource>` due to it not being object-safe.
    fn get_resource(&self, name: &str) -> Option<Box<dyn Any>>;
}
