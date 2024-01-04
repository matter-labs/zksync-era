use anyhow::Context as _;

/// Trait reverse to `zksync_protobuf::ProtoFmt` for cases where
/// you would like to specify a custom proto encoding for an externally defined type.
pub(crate) trait ProtoRepr:
    zksync_protobuf::build::prost_reflect::ReflectMessage + Default
{
    type Type;
    fn read(&self) -> anyhow::Result<Self::Type>;
    fn build(this: &Self::Type) -> Self;
}

pub(crate) fn read_required_repr<P: ProtoRepr>(field: &Option<P>) -> anyhow::Result<P::Type> {
    field.as_ref().context("missing field")?.read()
}
