use crate::{resource::Resource, service::context::ServiceContext, wiring_layer::WiringError};

/// Trait used as input for wiring layers, aiming to provide all the resources the layer needs for wiring.
///
/// For most cases, the most conevenient way to implement this trait is to use the `#[derive(FromContext)]`.
/// Otherwise, the trait has several blanket implementations (including the implementation for `()` and `Option`).
///
/// # Example
///
/// ```
/// use zksync_node_framework::FromContext;
/// # #[derive(Clone)]
/// # struct MandatoryResource;
/// # impl zksync_node_framework::resource::Resource for MandatoryResource { fn name() -> String { "a".into() } }
/// # #[derive(Clone)]
/// # struct OptionalResource;
/// # impl zksync_node_framework::resource::Resource for OptionalResource { fn name() -> String { "b".into() } }
/// # #[derive(Default, Clone)]
/// # struct ResourceWithDefault;
/// # impl zksync_node_framework::resource::Resource for ResourceWithDefault { fn name() -> String { "c".into() } }
/// #[derive(FromContext)]
/// struct MyWiringLayerInput {
///     // The following field _must_ be present in the context.
///     mandatory_resource: MandatoryResource,
///     // The following field is optional.
///     // If will be `None` if there is no such resource in the context.
///     optional_resource: Option<OptionalResource>,
///     // The following field is guaranteed to fetch the value from the context.
///     // In case the value is missing, a default value will be added to the context.
///     #[context(default)]
///     resource_with_default: ResourceWithDefault,
/// }
/// ```
pub trait FromContext: Sized {
    fn from_context(context: &mut ServiceContext<'_>) -> Result<Self, WiringError>;
}

impl<T: Resource + Clone> FromContext for T {
    fn from_context(context: &mut ServiceContext<'_>) -> Result<Self, WiringError> {
        context.get_resource::<T>()
    }
}

impl FromContext for () {
    fn from_context(_context: &mut ServiceContext<'_>) -> Result<Self, WiringError> {
        Ok(())
    }
}

impl<T: FromContext> FromContext for Option<T> {
    fn from_context(context: &mut ServiceContext<'_>) -> Result<Self, WiringError> {
        match T::from_context(context) {
            Ok(inner) => Ok(Some(inner)),
            Err(WiringError::ResourceLacking { .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// Trait used as output for wiring layers, aiming to provide all the resources and tasks the layer creates.
///
/// For most cases, the most conevenient way to implement this trait is to use the `#[derive(IntoContext)]`.
/// Otherwise, the trait has several blanket implementations (including the implementation for `()` and `Option`).
/// Note, however, that due to the lack of specialization, the blanket implementation for `Option<T: Task>` is not
/// provided. When used in the macro, tasks must be annotated with the `#[context(task)]` attribute.
///
/// Note: returning a resource that already exists in the context will result in a wiring error. If you need to provide
/// a "substitute" resource, request `Option` of it in the `FromContext` implementation to check whether it's already
/// provided.
///
///
/// # Example
///
/// ```
/// use zksync_node_framework::IntoContext;
/// # struct MyTask;
/// # #[async_trait::async_trait]
/// # impl zksync_node_framework::task::Task for MyTask {
/// #   fn id(&self) -> zksync_node_framework::TaskId { "a".into() }
/// #   async fn run(self: Box<Self>, _: zksync_node_framework::StopReceiver) -> anyhow::Result<()> { Ok(()) }
/// # }
/// # struct MaybeTask;
/// # #[async_trait::async_trait]
/// # impl zksync_node_framework::task::Task for MaybeTask {
/// #   fn id(&self) -> zksync_node_framework::TaskId { "b".into() }
/// #   async fn run(self: Box<Self>, _: zksync_node_framework::StopReceiver) -> anyhow::Result<()> { Ok(()) }
/// # }
/// # struct MyResource;
/// # impl zksync_node_framework::resource::Resource for MyResource { fn name() -> String { "a".into() } }
/// # struct MaybeResource;
/// # impl zksync_node_framework::resource::Resource for MaybeResource { fn name() -> String { "b".into() } }
/// #[derive(IntoContext)]
/// struct MyWiringLayerOutput {
///     // This resource will be inserted unconditionally.
///     // Will err if such resource is already present in the context.
///     recource: MyResource,
///     // Will only provide the resource if it's `Some`.
///     maybe_resource: Option<MaybeResource>,
///     // Will provide task unconditionally.
///     #[context(task)]
///     task: MyTask,
///     // Will provide task only if it's `Some`.
///     #[context(task)]
///     maybe_task: Option<MaybeTask>,
/// }
/// ```
pub trait IntoContext {
    fn into_context(self, context: &mut ServiceContext<'_>) -> Result<(), WiringError>;
}

// Unfortunately, without specialization we cannot provide a blanket implementation for `T: Task`
// as well. `Resource` is chosen because it also has a blanket implementation of `FromContext`.
impl<T: Resource> IntoContext for T {
    fn into_context(self, context: &mut ServiceContext<'_>) -> Result<(), WiringError> {
        context.insert_resource(self)
    }
}

impl IntoContext for () {
    fn into_context(self, _context: &mut ServiceContext<'_>) -> Result<(), WiringError> {
        Ok(())
    }
}

impl<T: IntoContext> IntoContext for Option<T> {
    fn into_context(self, context: &mut ServiceContext<'_>) -> Result<(), WiringError> {
        if let Some(inner) = self {
            inner.into_context(context)
        } else {
            Ok(())
        }
    }
}
