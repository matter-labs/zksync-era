# v31 Local Upgrade Testing Server Findings

This document records the necessity audit for the `zksync-era` side of the v29 -> v31 local upgrade-testing work.

Validated state:

- `zksync-era`: `2f3632dc9d551820212d2210363a987d4b1019a9`
- `contracts` submodule used by validation: `d248b3b0d3817ed53e801e56dc77c31c12538986`
- clean command:

```bash
PATH=/root/.cargo/bin:/root/tools/foundry-zksync-v0.1.5:$PATH \
  ./era-cacher/do-upgrade.sh 2>&1 | tee /root/upgrade_validation_current/current-e2e-clean.log
```

Result: `era-cacher/do-upgrade.sh` exited 0. The script completed the v31 chain upgrade, restarted the new server, and
ran the non-prividium integration suite successfully.

Integration result from the clean run:

```text
Test Suites: 1 skipped, 16 passed, 16 of 17 total
Tests:       4 skipped, 182 passed, 186 total
Integration tests ran successfully
```

Database evidence after the clean run:

```text
protocol_versions:
29 | 6c71a0efe8c4278aa14ed883397735792857f01b0fca184efcd6af92ea9165f5
31 | ccfcb39b4a74c81c55092d829677b6420aa4eeba46ec6c4679d87e86484e9112

l1_batches:
29 | 2  | 0..1
31 | 73 | 2..74

factory_deps count for TransparentUpgradeableProxy + BeaconProxy hashes: 2
```

## Executive Verdict

The server-side PR is fundamentally correct with one required local follow-up: the `contracts` submodule must point at
the final cleaned contracts commit `d248b3b0d3817ed53e801e56dc77c31c12538986`, because that is the contracts state
validated by the clean e2e run. I found one non-functional cleanup item and removed it: a stale inline "Removed:"
comment in `default_ecosystem_upgrade.rs`. I did not find behavioral changes that are safe to delete.

## Required Server Changes

### `contracts` submodule

Disposition: keep / update to `d248b3b0d3817ed53e801e56dc77c31c12538986`.

Why: the verified e2e flow depends on the final contracts cleanup shape: v31 Era system deps include
`SystemContractProxyAdmin`, and proxy runtime bytecodes are present as factory deps. Without that contracts state, the
server-side branch does not represent the tested flow.

Evidence:

- clean e2e run used this submodule commit.
- DB contains the expected `TransparentUpgradeableProxy` and `BeaconProxy` bytecode hashes after the upgrade.
- post-upgrade batches were produced with protocol version 31.

### `core/lib/contracts/src/lib.rs`

Disposition: keep.

Change: adds `EraSettlementLayerV31Upgrade` ABI loading.

Why: eth-watch must call the v31 on-chain helper `getL2UpgradeTxData(...)` on the actual v31 upgrader contract. Loading
the ABI from checked-in/generated contract artifacts avoids runtime JSON path guessing and avoids reimplementing
Solidity calldata logic off-chain.

Failure prevented: without this ABI, server code cannot call the on-chain v31 helper and must either keep placeholder tx
data or duplicate contract logic off-chain. The former does not produce the accepted v31 upgrade tx bytes; the latter is
a shortcut with drift risk.

Evidence:

- clean e2e persisted protocol version 31 with upgrade tx hash `ccfcb39b...`.
- post-upgrade server accepted the upgrade, restarted, and produced v31 batches.

### `core/lib/types/src/protocol_upgrade.rs`

Disposition: keep.

Change: extends `ProtocolUpgradePreimageOracle` with `rewrite_v31_upgrade_tx_data(init_address, existing_tx_data)` and
calls it only for protocol version 31 after decoding `DiamondCutData.initAddress`.

Why: v31 diamond-cut init calldata contains placeholder L2 tx data. The final L2 upgrade tx data is chain-specific and
must be derived by the v31 upgrader contract from live chain state: Bridgehub address, chain id, ZKsync OS flag, and the
existing tx data. This belongs at the same point where eth-watch reconstructs protocol upgrade transactions from
diamond-cut data.

Failure prevented: without the rewrite, eth-watch stores an upgrade tx whose bytes do not match the L1-accepted v31
upgrade path. That makes the local upgrade test pass only if it avoids validating the real upgrade tx.

Why this is not a shortcut: the Rust code delegates the v31-specific transformation to the deployed on-chain helper
instead of mirroring Solidity logic.

Evidence:

- clean e2e persisted v31 upgrade tx hash `ccfcb39b4a74c81c55092d829677b6420aa4eeba46ec6c4679d87e86484e9112`.
- the node restarted from the post-upgrade DB and produced protocol-version-31 L1 batches.

### `core/node/eth_watch/src/client.rs`

Disposition: keep.

Change: adds `get_l2_upgrade_tx_data(...)` to `EthClient`, loads the hyperchain ABI and v31 upgrader ABI, reads
`getBridgehub()` and `getZKsyncOS()` from the live diamond, then calls `getL2UpgradeTxData(...)` on
`DiamondCutData.initAddress`.

Why: the v31 rewrite requires live on-chain inputs. Reading them from the chain avoids hardcoded Bridgehub addresses and
avoids assuming Era vs ZKsync OS from local config. Using `initAddress` from the accepted diamond cut ties the call to
the exact upgrader contract selected by CTM.

Failure prevented: hardcoded or config-derived values can drift from the chain under test; off-chain guessed calldata
can differ from the L1-accepted bytes.

Evidence:

- clean e2e source of truth was the live local deployment, not static config.
- the runtime config had to be refreshed during the flow, and the upgraded node accepted the DB tx derived by eth-watch.

### `core/node/eth_watch/src/event_processors/decentralized_upgrades.rs`

Disposition: keep.

Change: wires the new oracle method through the eth-watch event processor.

Why: `ProtocolUpgrade::try_from_diamond_cut` is generic over the preimage oracle. The eth-watch processor is the
production/local component that has the L1 client needed to read bytecode preimages and call the v31 helper.

Failure prevented: the type-level protocol upgrade reconstruction would have no way to obtain v31 rewritten tx data.

Evidence: covered by the clean e2e run and post-upgrade protocol-version-31 batches.

### `core/node/eth_watch/src/tests/client.rs`

Disposition: keep.

Change: adds a pass-through mock implementation of `get_l2_upgrade_tx_data(...)`.

Why: required by the trait extension. The existing tests are not v31 rewrite tests; a pass-through preserves their
original semantics.

Failure prevented: test compilation failure after extending the trait.

### `zkstack_cli/crates/zkstack/src/admin_functions.rs`

Disposition: keep.

Change: `governance_execute_calls` returns default output after broadcast mode instead of attempting to read the
generated output file.

Why: broadcast execution is the real side effect for governance stages. In broadcast mode, the wrapper does not need the
prepared calldata output, and reading it after broadcast is fragile/unnecessary.

Failure prevented: governance stages can fail after successful on-chain execution due to missing or stale script output.

Evidence:

- clean e2e ran governance stages 0, 1, and 2 through broadcast mode.
- stage 1 updated CTM config with the new BytecodesSupplier after the governance call.

### `zkstack_cli/crates/zkstack/src/commands/chain/admin_call_builder.rs`

Disposition: keep.

Change: encodes `upgradeChainFromVersion` differently based on the old protocol version: pre-v31 chains use the legacy
two-argument Admin ABI, while v31+ chains use the current three-argument ABI including the chain address.

Why: the v29 source chain still has the pre-v31 Admin facet. The v31 ABI includes
`upgradeChainFromVersion(address,uint256,DiamondCutData)`, but the v29 Admin facet accepts
`upgradeChainFromVersion(uint256,DiamondCutData)`. The local v29 -> v31 flow must call the ABI that exists on the source
chain.

Failure prevented: the chain upgrade transaction would use the wrong selector against a v29 diamond and fail.

Evidence:

- clean e2e `run-chain-upgrade --upgrade-version v31-interop-b` completed.
- L1 accepted the upgrade call and the DB contains protocol version 31.

### `zkstack_cli/crates/zkstack/src/commands/dev/commands/upgrades/default_chain_upgrade.rs`

Disposition: keep.

Changes:

- remove the earlier `run_chain_upgrade_from_ctm` shortcut.
- restore the normal `run_chain_upgrade(...)` path for broadcast and dry-run.
- read Bridgehub from the v31 upgrade output shape under `deployed_addresses.bridgehub`.
- allow `chain_upgrade_diamond_cut` to be read from `chain_upgrade_diamond_cut_file`.

Why: the deleted helper bypassed the normal chain-upgrade command path by asking a contract-side helper to read from CTM
and execute the upgrade. That was useful as a debugging shortcut but not honest e2e testing. The final path uses the
generated upgrade description, parses the real diamond cut, schedules the upgrade through ChainAdmin, and executes the
chain upgrade through the normal admin-call builder.

Failure prevented:

- without the output-shape fix, v31 upgrade output cannot be parsed.
- without `chain_upgrade_diamond_cut_file`, very large diamond-cut data cannot be loaded from the generated artifact.
- without the normal path, the test would not prove the actual zkstack chain-upgrade wrapper behavior.

Evidence:

- clean e2e used `zkstack dev run-chain-upgrade --upgrade-version v31-interop-b ...`.
- the final logs show `Upgrade completed successfully!`.
- post-upgrade protocol version 31 and v31 batches are present in DB.

### `zkstack_cli/crates/zkstack/src/commands/dev/commands/upgrades/default_ecosystem_upgrade.rs`

Disposition: keep, with one comment removed as noise.

Changes:

- for v31 only, encode `noGovernancePrepare((...))` calldata from canonical config values.
- select Foundry broadcast file deterministically by calldata selector instead of assuming `run-latest.json`.
- update CTM BytecodesSupplier in saved contracts config after governance stage 1.

Why: v31 `noGovernancePrepare` requires a structured parameter object; the script output/broadcast filename changes when
Forge is invoked with explicit calldata. The BytecodesSupplier is replaced during stage 1 and must be reflected in
config before regenerating chain runtime config and restarting the node.

Failure prevented:

- broadcast lookup can read the wrong or nonexistent file.
- stale BytecodesSupplier in config causes post-upgrade server/runtime state to disagree with the deployed CTM state.
- manually supplied or hardcoded parameters would make local testing drift from the configured chain.

Evidence:

- clean e2e generated YAML using the selector-specific broadcast file.
- `update-permanent-values.sh` observed BytecodesSupplier changing from `0x3e9859...` to `0xc28d6e...`.
- post-upgrade server started and produced protocol-version-31 batches.

Cleanup taken:

- removed a stale inline comment: `Removed: LOCAL_GATEWAY_CHAIN_NAME - no longer needed with env var approach`.

### `infrastructure/local-upgrade-testing/era-cacher/do-upgrade.sh`

Disposition: keep.

Changes:

- make the script strict (`set -euo pipefail`) and path-independent.
- wait for L1 and L2 RPC readiness instead of fixed sleeps.
- deploy the deterministic CREATE2 factory when absent.
- keep the old-tree server running until the on-chain upgrade reaches the new ABI.
- update permanent values from the live deployment.
- generate v31 YAML from the selector-specific broadcast file.
- refresh generated chain configs after governance stage 1.
- execute the normal chain upgrade path.
- re-set the DA validator pair after v31 resets it.
- run stage3 with live Bridgehub from runtime config.
- create an empty bridged-token config only when the file is absent.
- restart the new server and wait for post-upgrade RPC.
- run the non-prividium integration suites with dependencies.

Why: this script is the e2e harness. Its job is to exercise the real local v29 -> v31 path, not only make a partial
script succeed. Each ordering step is tied to a real runtime dependency observed during testing.

Failure prevented:

- fixed sleeps race RPC readiness.
- missing CREATE2 factory breaks deterministic v31 deployments.
- starting the new server too early fails against pre-upgrade v29 chain state.
- stale config after governance stage 1 points the node at the wrong BytecodesSupplier.
- unset DA validator pair prevents post-upgrade batch commitments.
- hardcoded Bridgehub would drift from the actual local deployment.
- `--no-deps` integration testing does not prove the full post-upgrade test environment.

Evidence:

- clean `do-upgrade.sh` run exited 0.
- post-upgrade RPC became ready inside the script.
- integration tests ran from the script and passed.

### `infrastructure/local-upgrade-testing/era-cacher/reset.sh`

Disposition: keep.

Change: make reset path-independent, strict, quoted, and able to clean containers before rotating `zksync-working` into
the empty old/new checkout.

Why: the harness is invoked from its parent workspace and must not depend on the caller's current directory beyond the
expected layout. Cleaning containers before rotating avoids stale local services contaminating the next run.

Failure prevented: accidental use of the wrong checkout or stale containers.

### `infrastructure/local-upgrade-testing/era-cacher/update-permanent-values.sh`

Disposition: keep.

Change: read CTM, BytecodesSupplier, CREATE2 factory, and salt from canonical `configs/contracts.yaml`, not generated
`chains/era/configs/contracts.yaml`; then sync the generated runtime file with the live values.

Why: v31 governance stage 1 updates canonical contracts config. The generated chain config can lag until regenerated,
but the server reads `chains/era/configs/contracts.yaml` on restart. Both must be reconciled before the post-upgrade
server starts.

Failure prevented: post-upgrade server starts with stale contract addresses.

Evidence: clean run logs show the BytecodesSupplier update and successful post-upgrade restart.

### `infrastructure/local-upgrade-testing/era-cacher/use-new-era.sh`

Disposition: keep.

Change: make checkout switching path-independent, strict, quoted, and copy both `chains` and `configs` from old to new
before switching.

Why: the new checkout must inherit the live local deployment state generated by the old checkout. `configs` contains
canonical ecosystem contract state; `chains` contains per-chain runtime state.

Failure prevented: new-era commands run with missing or stale deployment/config files.

### `infrastructure/local-upgrade-testing/era-cacher/use-old-era.sh`

Disposition: keep.

Change: make checkout switching path-independent, strict, and quoted.

Why: same harness robustness reason as `use-new-era.sh`.

Failure prevented: accidental path expansion or wrong checkout moves.

## Shortcut Audit

Checked shortcut candidates:

- Off-chain v31 tx-data rewrite: absent. Rust calls the on-chain v31 upgrader helper.
- Runtime ABI filesystem guessing: absent. ABI is loaded via `zksync_contracts`.
- Hardcoded Bridgehub for stage3: absent. Script reads the live runtime config.
- Hardcoded BytecodesSupplier after governance: absent. Script and zkstack config are updated from generated stage
  output.
- Broadcast file guessed as `run-latest.json`: absent for v31. File is derived from the actual calldata selector.
- Contract-side helper replacing zkstack chain upgrade path: removed. The final chain upgrade uses `run_chain_upgrade`.
- Fixed sleeps as readiness proof: removed from the main flow. RPC polling is used.
- `--no-deps` integration shortcut: removed. The script runs integration tests with dependencies, excluding prividium by
  explicit suite list.
- Manually injected DA validator pair: present but required operator action. v31 stage resets the pair; the script reads
  the L1 validator address from runtime config and calls the normal zkstack command to re-set it.

## Residuals

- The `contracts` submodule pointer is currently a required local change unless committed on the server branch.
- Prividium tests are intentionally out of scope and are not part of the verified command.
- Consensus logs still report old v29 proposal mismatch warnings. They did not block RPC, post-upgrade batch production,
  or integration tests, and are not tied to the v31 upgrade correctness path validated here.
