# Data Availability Client

This crate contains a trait that has to be implemented by all the DA clients.

## Overview

This trait assumes that every implementation follows these logical assumptions:

- The DA client is only serving as a connector between the ZK chain's sequencer and the DA layer.
- The DA client is not supposed to be a standalone application, but rather a library that is used by the
  `da_dispatcher`.
- The logic of the retries is implemented in the `da_dispatcher`, not in the DA clients.
- The `dispatch_blob` is supposed to be idempotent, and work correctly even if called multiple times with the same
  params.
- The `get_inclusion_data` has to return the data only when the state roots are relayed to the L1 verification contract
  (if the DA solution has one).
