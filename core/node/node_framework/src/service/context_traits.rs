use crate::{service::context::ServiceContext, wiring_layer::WiringError};

pub trait FromContext: Sized {
    fn from_context(context: &mut ServiceContext<'_>) -> Result<Self, WiringError>;
}

pub trait IntoContext {
    fn into_context(self, context: &mut ServiceContext<'_>) -> Result<(), WiringError>;
}
