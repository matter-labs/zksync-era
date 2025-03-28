use std::{any::TypeId, fmt};

pub use self::{resource_id::ResourceId, unique::Unique};

mod resource_id;
mod unique;

/// A trait for anything that can be stored (and retrieved) as a resource.
///
/// Typically, the type that implements this trait also should implement `Clone`
/// since the same resource may be requested by several tasks and thus it would be an additional
/// bound on most methods that work with [`Resource`].
///
/// # Example
///
/// ```
/// # use zksync_node_framework::resource::Resource;
/// # use std::sync::Arc;
///
/// /// An abstract interface you want to share.
/// /// Normally you want the interface to be thread-safe.
/// trait MyInterface: 'static + Send + Sync {
///     fn do_something(&self);
/// }
///
/// /// Resource wrapper.
/// #[derive(Clone)]
/// struct MyResource(Arc<dyn MyInterface>);
///
/// impl Resource for MyResource {
///     fn name() -> String {
///         // It is a helpful practice to follow a structured naming pattern for resource names.
///         // For example, you can use a certain prefix for all resources related to a some component, e.g. `api`.
///         "common/my_resource".to_string()
///     }
/// }
/// ```
pub trait Resource: 'static + Send + Sync + std::any::Any {
    /// Invoked after the wiring phase of the service is done.
    /// Can be used to perform additional resource preparation, knowing that the resource
    /// is guaranteed to be requested by all the tasks that need it.
    fn on_resource_wired(&mut self) {}

    /// Returns the name of the resource.
    /// Used for logging purposes.
    fn name() -> String;
}

/// Internal, object-safe version of [`Resource`].
/// Used to store resources in the node without knowing their exact type.
///
/// This trait is implemented for any type that implements [`Resource`], so there is no need to
/// implement it manually.
pub(crate) trait StoredResource: 'static + std::any::Any + Send + Sync {
    /// An object-safe version of [`Resource::name`].
    fn stored_resource_id(&self) -> ResourceId;

    /// An object-safe version of [`Resource::on_resource_wired`].
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
        ResourceId::of::<T>()
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
