# Celestia client

---

This is an implementation of the Celestia client capable of sending the blobs to DA layer. Normally, the light client is
required to send the blobs to Celestia, but this implementation is capable of sending the blobs to DA layer directly.

This is a simplified and adapted version of astria's code, look
[here](https://github.com/astriaorg/astria/tree/main/crates/astria-sequencer-relayer) for original implementation.

The generated files are copied from
[here](https://github.com/astriaorg/astria/tree/main/crates/astria-core/src/generated), which is not perfect, but allows
us to use them without adding the proto files and the infrastructure to generate the `.rs`.

While moving the files, the `#[cfg(feature = "client")]` annotations were removed for simplicity, so client code is
available by default.

If there is a need to generate the files from the proto files, the `tools/protobuf-compiler` from astria's repo can be
used.
