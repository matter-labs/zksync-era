# Object Store

This crate provides the object storage abstraction that allows to get, put and remove binary blobs. The following
implementations are available:

- File-based store saving blobs as separate files in the local filesystem
- GCS-based store
- Mock in-memory store

Normally, these implementations are not used directly. Instead, a store trait object can be constructed based on the
[configuration], which can be provided explicitly or constructed from the environment. This trait object is what should
be used for dependency injection.

Besides the lower-level storage abstraction, the crate provides high-level typesafe methods to store (de)serializable
objects. Prefer using these methods whenever possible.

[configuration]: ../config
