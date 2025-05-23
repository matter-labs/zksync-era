# Contract Deployment Allowlist

This module restricts contract deployments on zkSync Era to a predefined list of approved addresses.

## Configuration

Configure via `api.web3_json_rpc.deployment_allowlist` in your node's YAML:

```YAML
api:
  web3_json_rpc:
    deployment_allowlist:
      http_file_url: "https://example.com/allowlist.json"
      refresh_interval_secs: 60
```

- `http_file_url` (required): HTTP(S) URL to the allowlist JSON.
- `refresh_interval_secs` (optional): Refresh interval in seconds (default: 300).

To disable, set:

```YAML
api:
  web3_json_rpc:
    deployment_allowlist: {}
```

---

## Allowlist Format

The JSON served at the specified URL file must contain:

```JSON
{
  "addresses": [
    "0x1234...abcd",
    "0xbeef...dead"
  ]
}
```

- Served as `application/json`.
- ETag headers supported (optional).

---

### How Allowlist Enforcement Works

When a transaction is submitted, the `WhitelistedDeployPoolSink` inspects the virtual machine (VM) execution output to
determine if it includes any contract deployments.

#### Detection Flow

1. The VM emits an event for each contract deployment:

   - `ContractDeployed(address deployer, bytes32 bytecodeHash, address contractAddress)`

2. These events are parsed from the `execution_output.events` field.

3. The transaction initiator is checked:

   If the initiator is in the allowlist, all deployments in the transaction are allowed.

   This enables nested deployments of any depth from allowlisted addresses.

   The transaction is forwarded to the mempool via `MasterPoolSink`.

4. If the initiator is not allowlisted and deployment events are found:

   - The deployer address (from topic[1]) is extracted.
   - The address is checked against the current in-memory allowlist (`SharedAllowList`).

   A. If the deployer is **not** in the allowlist:

   - The transaction is **rejected** with a `DeployerNotInAllowList` error.
   - A log is written with the deployer address and transaction hash.

   B. If all deployments are allowed:

   - The transaction is forwarded to the mempool via `MasterPoolSink`.

#### Example Check (Simplified)

- If transaction initiator is allowlisted → allow all deployments

- Otherwise, for each deployment event detected → extract deployer address
  - Check: `allowlist.contains(deployer_address)`
  - If not found → reject transaction
  - Otherwise → forward to mempool

## Behavior

- On tx submission:
  - If tx initiator is allowlisted, all deployments are permitted regardless of nested depth
  - Otherwise, each contract deployer must be individually allowlisted
- If not permited → tx is rejected.
- On refresh: allowlist is reloaded via HTTP. Skipped if `304 Not Modified`.

### ETag Support

To reduce unnecessary network usage, the allowlist fetcher supports HTTP **ETag** headers:

- When the allowlist is first fetched, the response’s `ETag` header is cached.
- On subsequent requests, the client includes the header:

  If-None-Match: "<etag-value>"

- If the allowlist has **not changed**, the server responds with:

  304 Not Modified

  and the in-memory list remains unchanged.

#### Notes

- ETag support is optional but recommended.
- If the server does **not** return an `ETag`, the allowlist will be re-fetched and re-parsed every interval.
- The ETag value is stored in memory only (not persisted across restarts).
