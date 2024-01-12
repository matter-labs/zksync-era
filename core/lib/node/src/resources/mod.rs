pub mod object_store;
pub mod pools;
pub mod state_keeper;
pub mod stop_receiver;

/// A marker trait for anything that can be stored (and retrieved) as a resource.
/// Requires `Clone` since the same resource may be requested by several tasks.
///
/// TODO: More elaborate docs.
pub trait Resource: 'static + Clone + std::any::Any {}
