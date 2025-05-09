use std::any::TypeId;

/// A unique identifier of a resource.
///
/// Internal representation is [`TypeId`], which is a 64-bit hash.
/// That is sufficient for our purposes, as even when using 2^16 different resources,
/// the chance of a hash collision occurring is about 1 in 2^32.
/// This [Stack overflow answer](https://stackoverflow.com/a/62667633) explains how to derive the likelihood.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceId(TypeId);

impl ResourceId {
    pub fn of<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}
