# Prover data directory

This directory contains the data required to run provers.

Currently, it has the following sub-directories:

- [keys](./keys/): Data required for proof generation. This data is mapped to a single protocol version.
- [historical_data](./historical_data/) Descriptors for the protocol versions used in the past.

## Keys directory

`keys` directory is used by various components in the prover subsystem, and it generally can contain two kinds of data:

- Small static files, like commitments, finalization hints, or verification keys.
- Big generated blobs, like setup keys.

Small static files are committed to the repository. Big files are expected to be downloaded or generated on demand. Two
important notices as of Sep 2024:

- Path to setup keys can be overridden via configuration.
- Proof compressor requires an universal setup file, named, for example, `setup_2^24.bin` or `setup_2^26.bin`. It's
  handled separately from the rest of the keys, e.g. it has separate configuration variables, and can naturally occur in
  the `$ZKSYNC_HOME/keys/setup` during development.
