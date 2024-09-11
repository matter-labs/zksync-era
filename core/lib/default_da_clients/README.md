# Default DA Clients

This crate contains the default implementations of the Data Availability clients. Default clients are maintained within
this repo because they are tightly coupled with the codebase, and would cause the circular dependency if they were to be
moved to the [hyperchain-da](https://github.com/matter-labs/hyperchain-da) repository.

Currently, the following DataAvailability clients are implemented:

- `NoDA client` that does not send or store any pubdata, it is needed to run the zkSync network in the "no-DA" mode
  utilizing the DA framework.
- `Object Store client` that stores the pubdata in the Object Store(GCS).
