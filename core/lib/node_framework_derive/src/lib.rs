extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

use crate::macro_impl::{MacroImpl, MacroKind};

pub(crate) mod helpers;
mod labels;
mod macro_impl;

/// Derive macro for the `FromContext` trait.
/// Allows to automatically fetch all the resources and tasks from the context.
///
/// # Example
///
/// ```
/// use zksync_node_framework_derive::FromContext;
/// # mod service {
/// #     pub struct ServiceContext<'a> {
/// #         _marker: std::marker::PhantomData<&'a ()>,
/// #     }
/// #     impl<'a> ServiceContext<'a> {
/// #         pub fn get_resource<T>(&mut self) -> Result<T, crate::WiringError> {
/// #             unimplemented!()
/// #         }
/// #         pub fn get_resource_or_default<T>(&mut self) -> T {
/// #             unimplemented!()
/// #         }
/// #     }
/// # }
/// # pub enum WiringError {
/// #     ResourceLacking { _name: String },
/// # }
/// # pub trait FromContext: Sized {
/// #     fn from_context(context: &mut service::ServiceContext<'_>) -> Result<Self, WiringError>;
/// # }
/// # struct MandatoryResource;
/// # struct OptionalResource;
/// # struct ResourceWithDefault;
/// #[derive(FromContext)]
/// # #[ctx(local)]
/// struct MyWiringLayerInput {
///     // The following field _must_ be present in the context.
///     mandatory_resource: MandatoryResource,
///     // The following field is optional.
///     // If will be `None` if there is no such resource in the context.
///     optional_resource: Option<OptionalResource>,
///     // The following field is guaranteed to fetch the value from the context.
///     // In case the value is missing, a default value will be added to the context.
///     #[resource(default)]
///     resource_with_default: ResourceWithDefault,
/// }
/// ```
#[proc_macro_derive(FromContext, attributes(ctx, resource, task))]
pub fn from_context_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    MacroImpl::parse(MacroKind::FromContext, input)
        .and_then(|from_context| from_context.render())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derive macro for the `IntoContext` trait.
/// Allows to automatically insert all the resources in tasks created by the wiring layer
/// into the context.
///
/// Both resources and tasks can be `Option`s. In that case, they will only be inserted if
/// provided.
///
/// Note: returning a resource that already exists in the context will result in a wiring
/// error. If you need to provide a "substitute" resource, request `Option` of it in the
/// `FromContext` implementation to check whether it's already provided.
///
/// # Example
///
/// ```
/// use zksync_node_framework_derive::IntoContext;
/// # mod service {
/// #     pub struct ServiceContext<'a> {
/// #         _marker: std::marker::PhantomData<&'a ()>,
/// #     }
/// #     impl<'a> ServiceContext<'a> {
/// #         pub fn add_task<T>(&mut self, _task: T) -> &mut Self {
/// #             unimplemented!()
/// #         }
/// #         pub fn insert_resource<T>(&mut self, _resource: T) -> Result<(), super::WiringError> {
/// #             unimplemented!()
/// #         }
/// #     }
/// # }
/// # pub enum WiringError {
/// #     ResourceLacking { _name: String },
/// # }
/// # pub trait IntoContext {
/// #     fn into_context(self, context: &mut service::ServiceContext<'_>) -> Result<(), WiringError>;
/// # }
/// # struct MyTask;
/// # struct MaybeTask;
/// # struct MyResource;
/// # struct MaybeResource;
/// #[derive(IntoContext)]
/// # #[ctx(local)]
/// struct MyWiringLayerOutput {
///     // This resource will be inserted unconditionally.
///     // Will err if such resource is already present in the context.
///     #[resource]
///     recource: MyResource,
///     // Will only provide the resource if it's `Some`.
///     #[resource]
///     maybe_resource: Option<MaybeResource>,
///     // Will provide task unconditionally.
///     #[task]
///     task: MyTask,
///     // Will provide task only if it's `Some`.
///     #[task]
///     maybe_task: Option<MaybeTask>,
/// }
/// ```
#[proc_macro_derive(IntoContext, attributes(ctx, resource, task))]
pub fn into_context_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    MacroImpl::parse(MacroKind::IntoContext, input)
        .and_then(|from_context| from_context.render())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
