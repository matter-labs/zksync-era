# General-Purpose Instrumentation for ZKsync

This library contains shared instrumentation tools used by ZKsync libraries.

Currently, the following tools are included:

- **Allocation monitoring** (`alloc` module). Provides a way to track (de)allocation for specific operations using
  Jemalloc (on the supported platforms).
- **Event filtering** (`filter` module). Efficient filtering of observable events.
