# Health Monitoring

Healthcheck infrastructure for node components allowing components to signal their current health state. Health states
for all components run by the node are aggregated and are exposed as an HTTP `GET /health` endpoint bound to a dedicated
healthcheck port, both for the main node and external node. This endpoint can be used as a readiness probe for
Kubernetes, or used in other automations.

## Main concepts

**Component** is a logically isolated part of a node that affects the ability of the node to handle requests (aka node
health). Components are supposed to run indefinitely until the node receives a stop signal.

- Internal components correspond to one or more Tokio tasks. Examples of internal components are: JSON-RPC API server,
  Merkle tree, consistency checker, reorg detector.
- External components correspond to another process that the node communicates with. Examples of external components
  are: Postgres connection pool, main node JSON-RPC (for the external node).

Each component can report its health, which consists of 2 parts:

- **Status**, e.g., "not ready", "ready", "shut down", "panicked"; see the crate code for a full list.
- **Details**, a JSON value with the component-specific schema. E.g., Merkle tree reports its L1 batch "cursor" as a
  part of this information.

Health from all components is aggregated into **application health**, which has its own status computed as the worst of
component statuses. Application health is returned by the `/health` endpoint.

## `/health` endpoint format

`/health` will return current application health encoded as a JSON object. The HTTP status of the response is 20x if the
application is healthy, and 50x if it is not.

> **Warning.** The schema of data returned by the `/health` endpoint is not stable at this point and can change without
> notice. Use at your own risk.

<details>
<summary>Example of endpoint output for an external node:</summary>

```json
{
  "status": "ready",
  "components": {
    "sync_state": {
      "status": "ready",
      "details": {
        "is_synced": true,
        "local_block": 91,
        "main_node_block": 91
      }
    },
    "connection_pool": {
      "status": "ready",
      "details": {
        "max_size": 50,
        "pool_size": 10
      }
    },
    "tree": {
      "status": "ready",
      "details": {
        "leaf_count": 12624,
        "mode": "full",
        "next_l1_batch_number": 26,
        "root_hash": "0x54d537798f9ebd1b6463e3773c3549a389709987d559fdcd8d402a652a33fb68",
        "stage": "main_loop"
      }
    },
    "snapshot_recovery": {
      "status": "ready",
      "details": {
        "factory_deps_recovered": true,
        "snapshot_l1_batch": 24,
        "snapshot_miniblock": 89,
        "storage_logs_chunk_count": 10,
        "storage_logs_chunks_left_to_process": 0,
        "tokens_recovered": true
      }
    },
    "consistency_checker": {
      "status": "ready",
      "details": {
        "first_checked_batch": 25,
        "last_checked_batch": 25
      }
    },
    "ws_api": {
      "status": "ready"
    },
    "prometheus_exporter": {
      "status": "ready"
    },
    "reorg_detector": {
      "status": "ready",
      "details": {
        "last_correct_l1_batch": 25,
        "last_correct_miniblock": 91
      }
    },
    "main_node_http_rpc": {
      "status": "ready"
    },
    "batch_status_updater": {
      "status": "ready",
      "details": {
        "last_committed_l1_batch": 25,
        "last_executed_l1_batch": 25,
        "last_proven_l1_batch": 25
      }
    },
    "commitment_generator": {
      "status": "ready",
      "details": {
        "l1_batch_number": 25
      }
    },
    "http_api": {
      "status": "ready"
    }
  }
}
```

</details>
