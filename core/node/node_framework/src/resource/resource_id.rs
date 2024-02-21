use std::{
    fmt,
    ops::{Add, AddAssign},
};

/// A unique identifier of the resource.
/// Typically, represented as a path-like string, e.g. `common/master_pool`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceId {
    /// Path-like representation of the resource identifier.
    /// Represented as a `Vec` for ID composability (e.g. collection IDs can be defined as
    /// `ResourceId::from("collection") + Resource::resource_id()`).
    id: Vec<&'static str>,
}

impl ResourceId {
    pub fn new(id: &'static str) -> Self {
        Self { id: vec![id] }
    }
}

impl Add<ResourceId> for ResourceId {
    type Output = Self;

    fn add(mut self, rhs: ResourceId) -> Self::Output {
        self.id.extend(rhs.id);
        self
    }
}

impl AddAssign<ResourceId> for ResourceId {
    fn add_assign(&mut self, rhs: ResourceId) {
        self.id.extend(rhs.id);
    }
}

impl From<&'static str> for ResourceId {
    fn from(id: &'static str) -> Self {
        Self { id: vec![id] }
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id.join("/"))
    }
}
