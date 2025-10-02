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

## S3

S3 implementation can be used to access different storages. Here is list of recommended values.

### GCS

See [details](https://cloud.google.com/storage/docs/authentication/managing-hmackeys)

- Endpoint: `https://storage.googleapis.com`
- Region: `us` or `auto`
- Access Key ID: Access key
- Secret Access Key: Corresponding secret

### R2

See [details](https://developers.cloudflare.com/r2/api/s3/tokens/)

- Endpoint: `https://<ACCOUNT_ID>.r2.cloudflarestorage.com`
- Region: `auto` or `us-east-1`
- Access Key ID: The id of the API token
- Secret Access Key: The SHA-256 hash of the API token value
