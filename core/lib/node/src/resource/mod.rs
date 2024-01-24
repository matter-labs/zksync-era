use std::{any::TypeId, fmt};

pub use self::{
    lazy_resource::LazyResource, resource_collection::ResourceCollection, resource_id::ResourceId,
};

mod lazy_resource;
mod resource_collection;
mod resource_id;

/// A trait for anything that can be stored (and retrieved) as a resource.
/// Typically, the type that implements this trait also should implement `Clone`
/// since the same resource may be requested by several tasks and thus it would be an additional
/// bound on most methods that work with [`Resource`].
pub trait Resource: 'static + Send + Sync + std::any::Any {
    /// Unique identifier of the resource.
    /// Used to fetch the resource from the provider.
    ///
    /// It is recommended to name resources in form of `<scope>/<name>`, where `<scope>` is the name of the task
    /// that will use this resource, or 'common' in case it is used by several tasks, and `<name>` is the name
    /// of the resource itself.
    fn resource_id() -> ResourceId;

    fn on_resource_wired(&mut self) {}
}

/// Internal, object-safe version of [`Resource`].
/// Used to store resources in the node without knowing their exact type.
///
/// This trait is implemented for any type that implements [`Resource`], so there is no need to
/// implement it manually.
pub trait StoredResource: 'static + std::any::Any + Send + Sync {
    /// An object-safe version of [`Resource::resource_id`].
    fn stored_resource_id(&self) -> ResourceId;

    /// An object-safe version of [`Resource::on_resoure_wired`].
    fn stored_resource_wired(&mut self);
}

impl<T: Resource> StoredResource for T {
    fn stored_resource_id(&self) -> ResourceId {
        T::resource_id()
    }

    fn stored_resource_wired(&mut self) {
        Resource::on_resource_wired(self);
    }
}

impl dyn StoredResource {
    /// Reimplementation of `Any::downcast_ref`.
    /// Returns `Some` if the type is correct, and `None` otherwise.
    // Note: This method is required as we cannot store objects as, for example, `dyn StoredResource + Any`,
    // so we don't have access to `Any::downcast_ref` within the node.
    pub(crate) fn downcast_ref<T: Resource>(&self) -> Option<&T> {
        if self.type_id() == TypeId::of::<T>() {
            // SAFETY: We just checked that the type is correct.
            unsafe { Some(&*(self as *const dyn StoredResource as *const T)) }
        } else {
            None
        }
    }
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
#[async_trait::async_trait]
pub trait ResourceProvider: 'static + Send + Sync + fmt::Debug {
    /// Returns a resource with the given name.
    ///
    /// In case it isn't possible to obtain the resource (for example, if some error occurred during initialization),
    /// the provider is free to either return `None` (if it assumes that the node can continue without this resource),
    /// or to panic.
    // Note: we have to use `Box<dyn Any>` here, since we can't use `Box<dyn Resource>` due to it not being object-safe.
    async fn get_resource(&self, resource: &ResourceId) -> Option<Box<dyn StoredResource>>;
}
