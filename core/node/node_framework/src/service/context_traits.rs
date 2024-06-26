use crate::{resource::Resource, service::context::ServiceContext, wiring_layer::WiringError};

pub trait FromContext: Sized {
    fn from_context(context: &mut ServiceContext<'_>) -> Result<Self, WiringError>;
}

impl<T> FromContext for Option<T>
where
    T: Resource + Clone,
{
    fn from_context(ctx: &mut ServiceContext<'_>) -> Result<Self, WiringError> {
        match ctx.get_resource::<T>() {
            Ok(resource) => Ok(Some(resource)),
            Err(WiringError::ResourceLacking { .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

pub trait IntoContext {
    fn into_context(self, context: &mut ServiceContext<'_>) -> Result<(), WiringError>;
}
