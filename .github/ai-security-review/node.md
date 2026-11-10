# Era Node Services Security Review Focus

Applies to RPC clients and servers, transaction admission, mempool and sealing, synchronization, snapshot recovery,
consensus, fee inputs, contract verification, prover-facing APIs, and background workers.

Apply the shared trust assumptions and evidence standards in the protocol focus. Follow service inputs into execution,
persisted state, and settlement, including behavior enabled by optional node roles or configuration.

## Priority checks

- **RPC and resource use:** Follow public HTTP/WebSocket methods, batching, subscriptions, filters, traces, and
  simulation into CPU, memory, database, and connection-pool costs. Check bounds, cancellation, and slow upstream calls
  while holding shared resources. Trace authentication and authorization for operator-only capabilities, and exposure of
  credentials or non-public node data. Public RPC endpoints intentionally expose public chain data. Missing rate limits
  alone do not establish an exploit.
- **Prover-facing APIs:** In `proof_data_handler`, `airbender_proof_data_handler`, and `external_proof_integration_api`,
  distinguish public proof-data/verification routes from privileged job operations. Authenticate and authorize
  privileged callers; bind job submissions to the assigned job, chain, batch, protocol, and proof stage. Check duplicate
  and stale submissions, claim/retry ownership, and limits on payload size, decompression, and verification work before
  persistence or job-state changes.
- **Consensus:** Check validator identity, key handling, and signature domains against the active chain and protocol.
  Trace membership, weights, `LeaderSelection`, and activation boundaries through `core/node/consensus/src/registry/`
  and configured schedules. Bind accepted payloads and acknowledgements to the intended block and validated execution
  result. Follow genesis, recovery, and membership transitions for skipped validation, stale authority, replay, or
  disagreement between stored state and acknowledged progress.
- **Fee inputs:** Trace external price sources through freshness checks, asset identity, units, decimal conversion,
  reciprocals, rounding, and arithmetic bounds. Check zero, negative, non-finite, stale, and extreme values against the
  configured policy. Follow feed outages, retries, and fallbacks into fee calculation, L1 ratio updates, transaction
  admission, and availability; establish attacker control before claiming underpricing or denial of service.
- **Upstream responses:** Bound raw and decompressed response bytes before buffering or allocation. Trace limits through
  shared HTTP middleware, streaming bodies, compression, redirects, timeouts, and retries. A downstream parser's size
  check does not protect an earlier unbounded collection or decompression. Follow the same client into synchronization,
  L1 queries, proof handling, and recovery to establish affected roles and attacker control of the upstream.
- **Mempool and sealing:** Preserve account/nonce ordering and consistent transaction classification through insertion,
  selection, replacement, stashing, rollback, rejection, and reinsertion. Check optional priority classes and capacity
  after garbage collection. Follow failure into lock poisoning, persisted retries, and restart behavior. Review sealing
  criteria together under the active geometry and fee model. Quantify disproportionate resource use or starvation; paid
  congestion and nearly full batches are not sufficient evidence.
- **State provenance and recovery:** Distinguish locally derived roots from peer or operator assertions repeated through
  another API, database row, or event. Require independent derivation when a component claims independent validation.
  Bind imported snapshot state and its pivot batch to the expected chain and trusted commitment before relying on later
  consistency checks. Check optional treeless/recovery modes, reorg detection, and peer authentication. Missing data or
  an indefinitely pending check must not become a successful validation or approval.
- **Persistence and worker lifecycle:** Preserve recoverability across Postgres, the Merkle tree, state-keeper cache,
  snapshots, and external effects. Follow consistency failures into stopping or rollback; validate rollback targets and
  ordering before destructive work. Review concurrent job claiming, retry ownership, idempotence, and crash recovery.
  Replaying stored input must not repeatedly poison workers or duplicate external effects.
- **Contract-verifier isolation:** Treat sources, imports, remappings, Standard JSON settings, filenames, and compiler
  selection as untrusted. Trace them into arguments, filesystem access, subprocesses, and diagnostics. Check alternate
  input formats and compiler backends; filtering imports alone does not constrain remappings. Inspect inherited
  environment, mounts, network access, and subprocess resource limits.
- **Contract-verification integrity:** Bind deployed factory-dependency hashes and linked child bytecode to submitted
  source and compiler settings before masking bytecode regions. Check creation code, constructor arguments, and
  immutable values, including factory deployments and differences between VM formats. Runtime equality alone does not
  establish how an EVM contract was initialized. Full-versus-partial results and final-record replacement rules must
  reflect which properties were authenticated. Valid hash syntax or link positions alone do not establish dependency
  identity.
- **Queue admission and capacity:** Connect request limits, deduplication, and per-submitter/per-contract throttles to
  global queue depth, compiler cost, worker concurrency, and retained input data. A per-job timeout does not bound queue
  growth or prevent repeated expensive submissions. Check admission, completion, failure, and retry transitions
  together; a failed request must not immediately bypass the intended cooldown or retain unbounded source data.
