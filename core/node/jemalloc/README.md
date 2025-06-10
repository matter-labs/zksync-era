# Jemalloc Tooling used by ZKsync Node

This library contains high-level tools for monitoring Jemalloc, the global memory allocator used by ZKsync nodes.

The main tool provided by the crate is `JemallocMonitorLayer`, which integrates into the
[node framework](../../lib/node_framework) and periodically collects Jemalloc stats as metrics and logs.
