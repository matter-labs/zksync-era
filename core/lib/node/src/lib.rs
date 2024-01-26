//! # ZK Stack node initialization framework.
//!
//! ## Introduction
//!
//! This crate provides core abstractions that allow one to compose a ZK Stack node.
//! Main concepts used in this crate are:
//! - [`IntoZkSyncTask`](task::IntoZkSyncTask) - builder interface for tasks.
//! - [`ZkSyncTask`](task::ZkSyncTask) - a unit of work that can be executed by the node.
//! - [`Resource`](resource::Resource) - a piece of logic that can be shared between tasks. Most resources are
//!   represented by generic interfaces and also serve as points of customization for tasks.
//! - [`ResourceProvider`](resource::ResourceProvider) - a trait that allows one to provide resources to the node.
//! - [`ZkSyncNode`](node::ZkSyncNode) - a container for tasks and resources that takes care of initialization, running
//!   and shutting down.
//!
//! The general flow to compose a node is as follows:
//! - Create a [`ResourceProvider`](resource::ResourceProvider) that can provide all the resources that the node needs.
//! - Create a [`ZkSyncNode`](node::ZkSyncNode) with that [`ResourceProvider`](resource::ResourceProvider).
//! - Add tasks to the node.
//! - Run it.

pub mod implementations;
pub mod node;
pub mod resource;
pub mod task;
