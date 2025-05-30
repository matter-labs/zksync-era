use std::{any::TypeId, fmt, sync::Arc};

pub use self::{resource_id::ResourceId, unique::Unique};
use crate::sealed::Sealed;

mod resource_id;
mod unique;

/// Marker trait for [`Resource`] kinds. The kind determines wrapper for a resource: no wrapper, `Arc` or `Box`.
pub trait ResourceKind: Sealed {}

/// Plain resource: no wrapper.
#[derive(Debug)]
pub struct Plain(());

impl Sealed for Plain {}
impl ResourceKind for Plain {}

/// Shared resource, wrapped in an [`Arc`].
#[derive(Debug)]
pub struct Shared(());

impl Sealed for Shared {}
impl ResourceKind for Shared {}

/// Boxed resource, wrapped in a [`Box`].
#[derive(Debug)]
pub struct Boxed(());

impl Sealed for Boxed {}
impl ResourceKind for Boxed {}

/// A trait for anything that can be stored (and retrieved) as a resource.
///
/// Typically, the type that implements this trait also should implement `Clone`
/// since the same resource may be requested by several tasks and thus it would be an additional
/// bound on most methods that work with [`Resource`].
///
/// # Example
///
/// ```
/// # use zksync_node_framework::resource::{Resource, Shared};
/// # use std::sync::Arc;
///
/// /// An abstract interface you want to share.
/// /// Normally you want the interface to be thread-safe.
/// trait MyInterface: 'static + Send + Sync {
///     fn do_something(&self);
/// }
///
/// impl Resource<Shared> for dyn MyInterface {
///     fn name() -> String {
///         // It is a helpful practice to follow a structured naming pattern for resource names.
///         // For example, you can use a certain prefix for all resources related to a some component, e.g. `api`.
///         "common/my_resource".to_string()
///     }
/// }
///
/// // The resource can now be injected / requested as `Arc<dyn MyInterface>`
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `{Kind}` resource",
    note = "If `Resource` is implemented for `{Self}`, check its type param: `Plain` (default), `Shared` or `Boxed`",
    note = "`Boxed` resources must be wrapped in a `Box` and `Shared` resources in an `Arc` when used in dependency injection"
)]
pub trait Resource<Kind: ResourceKind = Plain>: 'static + Send + Sync {
    /// Returns the name of the resource.
    /// Used for logging purposes.
    fn name() -> String;
}

impl<T: Resource<Shared> + ?Sized> Resource for Arc<T> {
    fn name() -> String {
        T::name()
    }
}

impl<T: Resource<Boxed> + ?Sized> Resource for Box<T> {
    fn name() -> String {
        T::name()
    }
}

/// Internal, object-safe version of [`Resource`].
/// Used to store resources in the node without knowing their exact type.
///
/// This trait is implemented for any type that implements [`Resource`], so there is no need to
/// implement it manually.
pub(crate) trait StoredResource: 'static + Send + Sync + std::any::Any {
    /// An object-safe version of [`Resource::name`].
    fn stored_resource_id(&self) -> ResourceId;
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
