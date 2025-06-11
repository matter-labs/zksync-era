use smart_config::{
    de::{DeserializeContext, DeserializeParam, WellKnown},
    metadata::{BasicTypes, ParamMetadata, TypeDescription},
    ErrorWithOrigin,
};

/// Combines the standard deserializer with another one having lower priority (wrapped by the type).
/// The fallback deserializer will only be invoked if the standard deserializer fails. If both deserializers fail,
/// both their errors will be reported.
#[derive(Debug)]
pub(crate) struct Fallback<De>(pub(crate) De);

impl<T, De> DeserializeParam<T> for Fallback<De>
where
    T: 'static + WellKnown,
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = <T::Deserializer>::EXPECTING.or(De::EXPECTING);

    fn describe(&self, description: &mut TypeDescription) {
        T::DE.describe(description);
        description.set_fallback(&self.0);
    }

    fn deserialize_param(
        &self,
        mut ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let main_err = match T::DE.deserialize_param(ctx.borrow(), param) {
            Ok(value) => return Ok(value),
            Err(err) => err,
        };
        self.0
            .deserialize_param(ctx.borrow(), param)
            .map_err(|fallback_err| {
                // Push both errors into the context.
                ctx.push_error(fallback_err);
                main_err
            })
    }

    fn serialize_param(&self, param: &T) -> serde_json::Value {
        // The main deserializer always has priority
        self.0.serialize_param(param)
    }
}
