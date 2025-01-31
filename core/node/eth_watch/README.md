# ZKsync Era Eth Watcher

This crate contains an implementation of the ZKsync Era Eth Watcher component, which fetches the changes from the
corresponding L1 contract.

## Overview

Internally, Eth Watcher contains _event processors_, each of which provides a relevant topic (i.e., a `bytes32` Solidity
event selector) and is responsible for processing the corresponding events. Besides events, processors have access to
the L1 client (to query more info) and to the node Postgres (to persist processing results). Examples of processors are:

- [Priority operations processor](src/event_processors/priority_ops.rs): persists priority operations (aka L1
  transactions)
- [Upgrades processor](src/event_processors/governance_upgrades.rs): persists protocol upgrades.

Eth Watcher combines topics from the processors into a single filter and periodically queries L1 for the corresponding
events. The fetched events are partitioned per processor and fed to them in succession.
