//! # ZK Stack node initialization framework.
//!
//! ## Introduction
//!
//! This crate provides core abstractions that allow one to compose a ZK Stack node.
//! Main concepts used in this crate are:
//! - [`WiringLayer`](wiring_layer::WiringLayer) - builder interface for tasks.
//! - [`Task`](task::Task) - a unit of work that can be executed by the node.
//! - [`Resource`](resource::Resource) - a piece of logic that can be shared between tasks. Most resources are
//!   represented by generic interfaces and also serve as points of customization for tasks.
//! - [`ResourceProvider`](resource::ResourceProvider) - a trait that allows one to provide resources to the node.
//! - [`ZkStackService`](service::ZkStackService) - a container for tasks and resources that takes care of initialization, running
//!   and shutting down.
//!
//! The general flow to compose a node is as follows:
//! - Create a [`ResourceProvider`](resource::ResourceProvider) that can provide all the resources that the node needs.
//! - Create a [`ZkStackService`](node::ZkStackService) with that [`ResourceProvider`](resource::ResourceProvider).
//! - Add tasks to the node.
//! - Run it.

pub mod implementations;
pub mod resource;
pub mod service;
pub mod task;
pub mod wiring_layer;
