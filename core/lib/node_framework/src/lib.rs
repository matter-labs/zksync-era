//! # ZK Stack node initialization framework.
//!
//! This crate provides core abstractions that allow one to compose a ZK Stack node.
//! Main concepts used in this crate are:
//! - [`WiringLayer`](wiring_layer::WiringLayer) - builder interface for tasks.
//! - [`Task`](task::Task) - a unit of work that can be executed by the node.
//! - [`Resource`](resource::Resource) - a piece of logic that can be shared between tasks. Most resources are
//!   represented by generic interfaces and also serve as points of customization for tasks.
//! - [`ZkStackService`](service::ZkStackService) - a container for tasks and resources that takes care of initialization, running
//!   and shutting down.
//! - [`ZkStackServiceBuilder`](service::ZkStackServiceBuilder) - a builder for the service.

pub mod resource;
pub mod service;
pub mod task;
pub mod wiring_layer;

/// Derive macro for the `FromContext` trait.
pub use zksync_node_framework_derive::FromContext;
/// Derive macro for the `IntoContext` trait.
pub use zksync_node_framework_derive::IntoContext;

pub use self::{
    resource::Resource,
    service::{FromContext, IntoContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};
