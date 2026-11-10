# Era Protocol and Execution Security Review Focus

Applies to VM execution, storage witnesses, proving, signing, protocol upgrades, cross-chain messages, and settlement.
Use the node and CI focus files where changes cross into services, recovery, builds, or deployment.

## Shared trust assumptions

Trace RPC input through transaction admission, execution, persistence, proving, and L1 settlement. Establish the active
protocol version, node role, proving mode, data-availability mode, and deployment configuration. Follow changed wiring
through local execution and prover interfaces to establish which trust assumptions still hold.

Distinguish ordinary users, peers, operators, and malicious provers. Authority to sequence transactions, supply data, or
submit proofs does not authorize arbitrary state transitions. Proof verification must reject invalid transitions even
when the prover controls witness inputs. Apply these assumptions and the evidence standards below across all three focus
files; investigate risks beyond their examples.

## Priority checks

- **State transitions and accounting:** Check transaction ordering, nonce and signature validation, fee and gas
  accounting, refunds, pubdata limits, priority operations, and batch boundaries. Trace native account-abstraction and
  paymaster validation through execution, including replay domains, validation restrictions, and fee-payer binding.
  Compare admission, simulation, and actual execution. Failed execution must preserve required fee/nonce effects without
  retaining effects that should revert, advancing priority cursors incorrectly, or permitting replay.
- **VM semantics and prover integration:** Compare the execution backends selected for the same protocol version,
  including handwritten bytecode. Check far-call/return exceptions, fat-pointer flags and bounds, gas stipends,
  decommitment, transient storage, and snapshot rollback. Follow validation flags into the actual exception path.
  Compare committed effects, not only return values or traces. Check host/prover input serialization, integer widths,
  and chain/batch identity. Bind guest artifacts and verification keys to the intended protocol and prover release;
  trace dependency, artifact, and configuration changes through local call sites and expected outputs. Review
  arithmetic, precompile edge inputs, bytecode/hash validation, deployment, and privilege boundaries around bootloader
  and system-contract calls. Check native/EVM gas conversion and exceptional precompile paths preserve the caller's
  intended gas cap.
- **Storage witnesses and pubdata:** Bind every VM-observed read to the authenticated pre-state, including absent keys
  and reads later collapsed into writes. Check zero values for missing leaves, first-write flags, enumeration indices,
  previous values, and duplicate-key precedence. Derive these from verified data or cross-check independent inputs.
  Trace conversion and deduplication for discarded fields; require resulting state diffs and pubdata to reconstruct the
  same state the proof commits.
- **Circuit constraints and commitments:** In reachable circuit or verifier changes, trace carries, borrows, range
  checks, boolean flags, and helper outputs into enforced constraints. Recheck fallback gates when gate configuration
  changes. Check that serialized commitments bind every security-relevant field, including running accumulators and
  state passed between circuit instances. Honest witness generation and debug assertions cannot substitute for
  constraints against a malicious prover.
- **Proof routing and upgrades:** Bind chain, batch order, old/new roots, protocol version, verification key, system
  bytecode, and public inputs through proof aggregation to settlement. Compare verified public inputs with locally
  expected batch commitments before acceptance and persistence; matching separately supplied metadata is insufficient.
  Check empty/optional values, canonical encoding, and domain separation across versions. Establish whether configured
  proving requires both backends, either backend, or one; verify failure propagation and authority to change modes.
  Trace upgrade activation, fallback, and recovery through verifier selection and settlement. Rejected proofs and failed
  submissions must leave a usable retry path, including after persisted work is replayed on restart.
- **Data availability and cross-chain messages:** Check rollup/validium configuration, blob/calldata or external DA
  validation, and whether published data matches commitments and supports recovery. Authenticate L1 event origins,
  contract addresses, chain IDs, confirmations, and ordering. Bind deposits, withdrawals, and imported message roots to
  their source chain, sender, recipient, payload, and inclusion proof. Follow replay protection and failed-operation
  recovery across Gateway and direct settlement paths; retries must not duplicate transfers or skip required messages.
- **L1 priority operations and proving capacity:** Trace authenticated events, ordering, replay protection, and
  commit/prove/execute retries. Compare admission for ordinary L2 transactions with forced L1-to-L2 operations and
  authorized upgrade/service transactions. Assess whole-batch cycles and memory, including distinct bytecode, storage
  witnesses, call stacks, and retained returndata. A per-transaction gas cap or sequencer-only guard may not bound
  execution or proving resources. Establish whether rejection, resealing, or the configured fallback can actually
  recover.
- **Signing and protocol tooling:** Bind signed operations to chain, account, nonce, destination, value, and calldata.
  Trace remote-signer authorization, key handling, upgrade calldata, and CLI-generated configuration into the executed
  operation. Review dependency revisions alongside local call sites, protocol interfaces, and expected outputs. Keep
  credentials out of logs and artifacts.

## Avoid false positives

- Public RPC endpoints on public chains intentionally expose public chain data. Check exposure of credentials,
  non-public node data, and operator-only controls. Public RPC remains in scope for resource-exhaustion attacks.
- Operator-selected values can be intentional, and bootloader or settlement checks may enforce an apparent missing local
  invariant. Verify downstream enforcement and the specific authority granted; operational trust does not make arbitrary
  proof witnesses trustworthy. Execution delays and monitoring mitigate impact without repairing soundness.
- Trace the reachability of suspected invalid states through callers and data-structure invariants. Check per-account
  queue uniqueness and reinsertion before claiming lost transactions. Partial cache rollback can be intentional when
  authoritative data can rebuild it.
- Separate accepted invalid state, rejected valid batches, bounded operational failures, and harmless trace differences.
  A malformed operator witness that fails closed is not automatically public DoS or forged-proof acceptance.
- Apply accepted limits only to the same actor, chain configuration, and impact. Tie impact to configured proving modes
  and resource limits. Known or deferred issues are not automatically accepted.

## Evidence and findings

Base findings on a traced code path that violates a security invariant. Report concrete `file:line` evidence, attacker
prerequisites, the affected node role and protocol version, the exploit path, impact, and a focused fix. Ground each
attack step in code, schemas, or configuration. Identify whether introduced, activated/widened, or an incomplete
remediation. Grade severity by impact and reachability, with confidence separate. Deduplicate root causes and keep
test-efficacy issues separate from production vulnerabilities.

For tests added or changed in the PR, check independent expected results, realistic privileges, valid positive controls,
and exact failure assertions. Check whether their setup and assertions cover the relevant malicious witnesses,
release-build behavior, host/prover integration, backend variants, restart paths, and actual worker completion. A queued
job, disabled worker, unrelated rejection, or incomplete scan cannot establish that a boundary works.
