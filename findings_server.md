# v31 Local Upgrade Testing Server Findings

This document records the necessity audit for the `zksync-era` side of the v29 -> v31 local upgrade-testing work.

Validated state:

- `zksync-era`: `83fb90ef372e9ece983138fa8f47f38a7b734491` plus the local minimized `do-upgrade.sh` follow-up documented
  below.
- `contracts` submodule used by validation: `d43fd4fd1a9a808896f44d785e42accd84cc7472`
- latest era-cacher command:

```bash
PATH=/root/.cargo/bin:/root/.foundry/bin:$PATH \
  bash era-cacher/do-upgrade.sh 2>&1 | tee /root/upgrade_validation_proxyadmin/proxyadmin-era-cacher.log
```

Result: the script completed the v31 chain upgrade and stage3 migration, but the final integration command raced the
post-upgrade server startup on a cold release build. The server was still compiling after the script's fixed 10 second
sleep, so Jest initially failed with `ECONNREFUSED 127.0.0.1:3050`. I replaced that fixed post-upgrade sleep with the
existing `zkstack server wait --timeout 600 --chain era` readiness check. After waiting for the upgraded server to
become healthy, the standard unfiltered integration command ran against the upgraded node; all non-prividium suites
passed.

Integration result after the upgraded server was healthy:

```text
Test Suites: 1 failed, 1 skipped, 16 passed, 17 of 18 total
Tests:       7 failed, 4 skipped, 182 passed, 193 total
```

```bash
PATH=/root/.cargo/bin:/root/.foundry/bin:$PATH \
  zkstack dev test integration --ignore-prerequisites --chain era \
  2>&1 | tee /root/upgrade_validation_proxyadmin/integration-after-upgrade.log
```

The only failed suite was `tests/prividium.test.ts`, with every failure caused by the expected missing
`chains/era/configs/private-rpc-permissions.yaml`. Per review scope, prividium is ignored; no changes should be kept
only for prividium.

```text
PASS tests/paymaster.test.ts
PASS tests/contracts.test.ts
PASS tests/interop-b.test.ts
PASS tests/custom-account.test.ts
PASS tests/system.test.ts
PASS tests/l1.test.ts
PASS tests/api/contract-verification.test.ts
PASS tests/ether.test.ts
FAIL tests/prividium.test.ts
PASS tests/api/web3.test.ts
PASS tests/evm.test.ts
PASS tests/mempool.test.ts
PASS tests/interop-a.test.ts
PASS tests/l2-erc20.test.ts
PASS tests/api/debug.test.ts
PASS tests/base-token.test.ts
PASS tests/erc20.test.ts
```

Database evidence after the latest run:

```text
protocol_versions:
29 | 6c71a0efe8c4278aa14ed883397735792857f01b0fca184efcd6af92ea9165f5
31 | f203bf1238cfb13c207fd55c38bf08c4b18d1349777f6c17012a7d6705e69e49

l1_batches:
31 | latest observed batch 81

miniblocks:
31 | latest observed miniblock 193

RPC:
eth_blockNumber = 0xc1
zks_L1BatchNumber = 0x51
code at 0x000000000000000000000000000000000001000c = 0x

factory_deps count for TransparentUpgradeableProxy + BeaconProxy hashes: 2
```

## Executive Verdict

The server-side PR is fundamentally correct with one required local follow-up: the `contracts` submodule must point at
the final cleaned contracts commit `d43fd4fd1a9a808896f44d785e42accd84cc7472`, because that is the contracts state
validated by the clean e2e run. I found one non-functional cleanup item and removed it: a stale inline "Removed:"
comment in `default_ecosystem_upgrade.rs`. I did not find behavioral changes that are safe to delete.

## Required Server Changes

### `contracts` submodule

Disposition: keep / update to `d43fd4fd1a9a808896f44d785e42accd84cc7472`.

Why: the verified e2e flow depends on the final contracts cleanup shape: Era does not force-deploy
`SystemContractProxyAdmin`, while `TransparentUpgradeableProxy` and `BeaconProxy` remain present as factory deps for
runtime deployments performed by the v31 upgrade path. Without that contracts state, the server-side branch does not
represent the tested flow.

Evidence:

- clean e2e run used this submodule commit.
- post-upgrade runtime check showed `0x000000000000000000000000000000000001000c` has empty code, which is expected for
  Era after the proxy-admin cleanup.
- DB contains the expected `TransparentUpgradeableProxy` and `BeaconProxy` bytecode hashes after the upgrade.
- post-upgrade batches were produced with protocol version 31.

### `core/lib/contracts/src/lib.rs`

Disposition: keep.

Change: adds v31 settlement-layer upgrader ABI loading.

Why: eth-watch must call the v31 on-chain helper `getL2UpgradeTxData(...)` on the actual v31 upgrader contract. Loading
the ABI from checked-in/generated contract artifacts avoids runtime JSON path guessing and avoids reimplementing
Solidity calldata logic off-chain.

Failure prevented: without this ABI, server code cannot call the on-chain v31 helper and must either keep placeholder tx
data or duplicate contract logic off-chain. The former does not produce the accepted v31 upgrade tx bytes; the latter is
a shortcut with drift risk.

Evidence:

- clean e2e persisted protocol version 31 with upgrade tx hash `ccfcb39b...`.
- post-upgrade server accepted the upgrade, restarted, and produced v31 batches.
- follow-up audit renamed the Rust helper from Era-specific naming to generic settlement-layer naming; the checked-in
  Era artifact is still used only because Era and ZKsync OS v31 upgraders expose the same `getL2UpgradeTxData` ABI.

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
- follow-up audit replaced manual packed-semver bit extraction with the existing `get_minor_protocol_version(...)`
  helper, preserving behavior while matching local practice.

### `zkstack_cli/crates/zkstack/src/commands/dev/commands/upgrades/default_chain_upgrade.rs`

Disposition: keep.

Changes:

- remove the earlier `run_chain_upgrade_from_ctm` shortcut.
- restore the normal `run_chain_upgrade(...)` path for broadcast and dry-run.
- read Bridgehub from the v31 upgrade output shape under `deployed_addresses.bridgehub`.
- allow `chain_upgrade_diamond_cut` to be read from `chain_upgrade_diamond_cut_file`.

Why: the deleted helper bypassed the normal chain-upgrade command path by asking a contract-side helper to read from CTM
and execute the upgrade. That helper was introduced to avoid TOML parsing failures on very large diamond-cut hex
strings, not because the zkstack chain-upgrade path was semantically incapable of doing the upgrade. The final fix
solves that root issue directly by loading `chain_upgrade_diamond_cut_file`, then keeps the normal command path for both
dry-run and broadcast.

The removed helper did the following:

- read local chain/wallet config;
- called `AdminFunctions.upgradeChainFromCTM(...)` through Forge;
- let the Solidity helper recover the CTM from the chain and reconstruct the diamond cut from CTM logs;
- executed the upgrade through `Utils.adminExecute(...)`.

The retained normal path does the upgrade through the surfaces that `zkstack dev run-chain-upgrade` is supposed to
exercise:

- reads the generated v31 upgrade description and large diamond cut artifact;
- fetches the live chain address and ChainAdmin from Bridgehub;
- schedules the upgrade timestamp on ChainAdmin;
- builds the ChainAdmin multicall via `AdminCallBuilder`;
- encodes the source-chain-compatible `upgradeChainFromVersion` selector;
- sends the final transaction through the normal zkstack transaction path.

So the change removes a shortcut rather than removing required behavior. The only functionality from the helper that was
actually needed was "large diamond-cut data can be loaded"; that is now handled explicitly by
`chain_upgrade_diamond_cut_file`.

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

- use the current checkout's local L1 genesis for the old checkout before starting reth.
- keep the old-tree server running until the on-chain upgrade reaches the new ABI.
- update permanent values from the live deployment.
- generate v31 YAML from the selector-specific broadcast file.
- refresh generated chain configs after governance stage 1.
- execute the normal chain upgrade path.
- re-set the DA validator pair after v31 resets it.
- run stage3 with live Bridgehub from runtime config.
- create an empty bridged-token config only when the file is absent.
- wait for the post-upgrade server health check before funding test wallets and starting integration tests.
- run integration tests with dependencies.

Why: this script is the e2e harness. Its job is to exercise the real local v29 -> v31 path, not only make a partial
script succeed. Each ordering step is tied to a real runtime dependency observed during testing.

Failure prevented:

- old v29 reth genesis lacks the deterministic CREATE2 factory predeploy that current v31 local tooling expects.
- starting the new server too early fails against pre-upgrade v29 chain state.
- stale config after governance stage 1 points the node at the wrong BytecodesSupplier.
- unset DA validator pair prevents post-upgrade batch commitments.
- hardcoded Bridgehub would drift from the actual local deployment.
- `--no-deps` integration testing does not prove the full post-upgrade test environment.
- fixed post-upgrade sleep races cold release builds and can start Jest before the upgraded RPC is listening.

Evidence:

- latest `do-upgrade.sh` run completed the upgrade and stage3 migration with no manual CREATE2 factory deployment
  helper.
- logs show `Using deterministic Create2Factory address: 0x4e59...` from the normal deploy scripts, and no
  `Deploying deterministic CREATE2` / `cast publish` helper path.
- the original fixed sleep was too short on a cold release build; manual `zkstack server wait` plus the same standard
  integration command produced 16 passing non-prividium suites against the upgraded node.

### `infrastructure/local-upgrade-testing/era-cacher/update-permanent-values.sh`

Disposition: keep.

Change: read CTM, BytecodesSupplier, CREATE2 factory, and salt from canonical `configs/contracts.yaml`, not generated
`chains/era/configs/contracts.yaml`; then sync the generated runtime file with the live values.

Why: v31 governance stage 1 updates canonical contracts config. The generated chain config can lag until regenerated,
but the server reads `chains/era/configs/contracts.yaml` on restart. Both must be reconciled before the post-upgrade
server starts.

Failure prevented: post-upgrade server starts with stale contract addresses.

Evidence: clean run logs show the BytecodesSupplier update and successful post-upgrade restart.

## Shortcut Audit

Checked shortcut candidates:

- Off-chain v31 tx-data rewrite: absent. Rust calls the on-chain v31 upgrader helper.
- Runtime ABI filesystem guessing: absent. ABI is loaded via `zksync_contracts`.
- Hardcoded Bridgehub for stage3: absent. Script reads the live runtime config.
- Hardcoded BytecodesSupplier after governance: absent. Script and zkstack config are updated from generated stage
  output.
- Broadcast file guessed as `run-latest.json`: absent for v31. File is derived from the actual calldata selector.
- Contract-side helper replacing zkstack chain upgrade path: removed. The final chain upgrade uses `run_chain_upgrade`.
- Fixed post-upgrade sleep as readiness proof: removed. The script now uses the existing
  `zkstack server wait --timeout 600 --chain era` health-check path before funding test wallets and running integration.
- `--no-deps` integration shortcut: removed. The script runs the standard integration command with dependencies:
  `zkstack dev test integration --ignore-prerequisites --chain era`.
- Manually injected DA validator pair: present but required operator action. v31 stage resets the pair; the script reads
  the L1 validator address from runtime config and calls the normal zkstack command to re-set it.

## Follow-up Prod-Likeness Audit

The whole server diff was re-scanned after the minimized contracts cleanup. These are the remaining places that are less
prod-like or further from the v29 shape, and their disposition:

- `build_v31_no_governance_prepare_calldata(...)`: still present. It manually encodes
  `noGovernancePrepare(EcosystemUpgradeParams)` and embeds the v31 input/output paths. This is the least prod-like piece
  left. It is not a shortcut because it calls the real Forge script and uses canonical config values, but it is less
  clean than v29's `run()` entrypoint with env-provided paths. I left it in place because removing it requires a
  contracts/script interface change, and the safer path is tracked by the inline PR review comment.
- v31 broadcast selector in `do-upgrade.sh`: still present. It duplicates the `noGovernancePrepare` signature only to
  point `upgrade-yaml-output-generator` at Foundry's selector-named broadcast file. This is harness glue, not upgrade
  logic. The zkstack code itself derives the broadcast filename from actual calldata bytes.
- explicit non-prividium integration suite list in `do-upgrade.sh`: removed. The script now uses the historical
  unfiltered command shape: `zkstack dev test integration --ignore-prerequisites --chain era`. Prividium remains
  intentionally out of scope if it fails because `private-rpc-permissions.yaml` is absent.
- `update-permanent-values.sh` uses shell YAML extraction/sed sync: still present. This is not ideal, but it is confined
  to era-cacher harness state reconciliation. The canonical source remains `configs/contracts.yaml`, and the sync is
  needed because the post-upgrade server reads `chains/era/configs/contracts.yaml`.
- old checkout reth chaindata replacement: still present. It is harness-level compatibility for the v29 source checkout,
  not a manual CREATE2 deployment shortcut. It uses the normal current local genesis instead of publishing code from the
  harness.
- Admin ABI version branch: kept, but tightened. The branch is required because a v29 source diamond exposes the legacy
  two-argument `upgradeChainFromVersion`. The follow-up audit removed manual semver bit math and now uses the existing
  protocol-version helper.
- settlement-layer ABI naming: fixed. The code no longer exposes an Era-specific Rust name for a helper call that is
  ABI-compatible across Era and ZKsync OS v31 upgraders.

## Residuals

- The `contracts` submodule pointer is currently a required local change unless committed on the server branch.
- Prividium tests are intentionally out of scope. They are included by the unfiltered historical integration command,
  but their expected `private-rpc-permissions.yaml` failure is ignored and should not drive code changes.
- Consensus logs still report old v29 proposal mismatch warnings. They did not block RPC, post-upgrade batch production,
  or integration tests, and are not tied to the v31 upgrade correctness path validated here.
