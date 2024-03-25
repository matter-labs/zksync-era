use std::{any::TypeId, fmt};

pub use self::{
    lazy_resource::LazyResource, resource_collection::ResourceCollection, resource_id::ResourceId,
    unique::Unique,
};

mod lazy_resource;
mod resource_collection;
mod resource_id;
mod unique;

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
pub(crate) trait StoredResource: 'static + std::any::Any + Send + Sync {
    /// An object-safe version of [`Resource::resource_id`].
    fn stored_resource_id(&self) -> ResourceId;

    /// An object-safe version of [`Resource::on_resoure_wired`].
    fn stored_resource_wired(&mut self);
}

impl fmt::Debug for dyn StoredResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resource")
            .field("resource_id", &self.stored_resource_id())
            .finish()
    }
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
