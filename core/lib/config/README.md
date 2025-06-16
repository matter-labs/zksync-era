# ZKsync Node Configuration

This crate provides configuration parameters for ZKsync nodes. Internally, it uses the [`smart-config`] library for
layered configuration with rich built-in metadata and some other nice features.

## Developer guidelines

- Where possible, config params should use simple types with a known serialization format (i.e., ones implementing
  [`WellKnown`] in terms of `smart-config`). Avoid params with a complex internal structure; consider splitting such
  params into multiple params.
- Do use collections like `Vec`, `HashSet` or `HashMap` where it's warranted. There are formats like [`NamedEntries`]
  (for deserialization from either an object, or an array of key–value pairs) for more complex cases.
- For params having a logical unit (e.g., a time duration or a byte size), **always** use `Duration` / [`ByteSize`] and
  the default deserializer; avoid creating params with a specific unit. (There are legacy params that do not follow this
  recommendation; their non-default deserialization will be deprecated and eventually removed.)
- When adding a new param, don't forget to update deserialization unit tests for the containing config (or create ones
  if they don't exist). Prefer using [`test_complete`] in such tests to ensure full param coverage.

[`smart-config`]: https://matter-labs.github.io/smart-config/smart_config/
[`WellKnown]: https://matter-labs.github.io/smart-config/smart_config/de/trait.WellKnown.html
[`NamedEntries`]: https://matter-labs.github.io/smart-config/smart_config/de/struct.NamedEntries.html
[`ByteSize`]: https://matter-labs.github.io/smart-config/smart_config/struct.ByteSize.html
[`test_complete`]: https://matter-labs.github.io/smart-config/smart_config/testing/fn.test_complete.html
