/**
 * TODO write a normal doc
 *
 * Main abstractions:
 * - Task
 * - Resource
 * - ResourceProvider
 * - ZkSyncNode
 *
 * Flow:
 *
 * You define a ResourceProvider that can provide all the resources that the node needs.
 * You create a ZkSyncNode with that ResourceProvider.
 * You add tasks to the node.
 *  
 * If needed, you add a specific healthcheck task to the node.
 *  `ZkSyncNode` will collect healtchecks from all the tasks you've added.
 * You run it.
 *
 */
pub mod healthcheck;
pub mod node;
pub mod resource;
pub mod task;
