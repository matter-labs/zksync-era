# ZK Stack Development Guidelines

## Rebuilding After Changes

### When to Rebuild zkstackup

**IMPORTANT:** After making changes to Rust code in the `zkstack_cli` directory, you must rebuild zkstackup for the
changes to take effect:

```bash
cd /path/to/zksync-working
zkstackup --local
```

This is necessary because:

- The `zkstack` command is a compiled binary installed in `~/.local/bin/`
- Changes to Rust source code won't take effect until the binary is rebuilt
- Without rebuilding, you'll be running the old version of the CLI

**When to rebuild:**

- After modifying any `.rs` files in `zkstack_cli/`
- After modifying Forge script parameters in `zkstack_cli/crates/config/src/forge_interface/script_params.rs`
- After making changes to upgrade command handlers

## Solidity Upgrade Scripts

### Ecosystem Upgrade Architecture

The v31 ecosystem upgrade uses a unified approach that combines both core contract upgrades and CTM (Chain Type Manager)
upgrades:

```
EcosystemUpgrade_v31
    ↓ extends
DefaultEcosystemUpgrade
    ↓ extends                    ↓ has instance of
DefaultCoreUpgrade          CTMUpgrade_v31
                                ↓ extends
                            DefaultCTMUpgrade
```

**Key Features:**

- `DefaultEcosystemUpgrade` runs both core and CTM upgrades sequentially
- Combines governance calls from both upgrades into unified stage0/1/2 calls
- Copies diamond cut data from CTM upgrade to ecosystem output file
- Avoids diamond inheritance conflicts by using composition (CTM upgrade as state variable)

**Files:**

- `deploy-scripts/upgrade/default_upgrade/DefaultEcosystemUpgrade.s.sol` - Base class for unified upgrades
- `deploy-scripts/upgrade/v31/EcosystemUpgrade_v31.s.sol` - v31-specific implementation
- `deploy-scripts/upgrade/v31/CTMUpgrade_v31.s.sol` - v31 CTM upgrade
- `deploy-scripts/upgrade/v31/CoreUpgrade_v31.s.sol` - Standalone core upgrade (not used by ecosystem upgrade)

**Environment Variables:**

- `V31_UPGRADE_ECOSYSTEM_OUTPUT` - Main output file path (e.g., `/script-out/v31-upgrade-core.toml`)
- `V31_UPGRADE_CTM_OUTPUT` - CTM output file path (e.g., `/script-out/v31-upgrade-ctm.toml`)

**Output Files:**

- The ecosystem output file (`v31-upgrade-core.toml`) contains:
  - Ecosystem contract addresses
  - CTM contract addresses
  - **Diamond cut data** for chain upgrades
  - Combined governance calls for all upgrade stages

## Debugging Forge Scripts

### Common Issues

1. **"call to non-contract address 0x0"**

   - Usually means a contract hasn't been deployed or registered yet
   - Check if required contracts exist at the expected addresses
   - For upgrades: ensure chains are registered before running upgrade scripts

2. **"vm.writeToml: path not allowed"**

   - Check that paths are correctly constructed (relative vs absolute)
   - Ensure `vm.projectRoot()` is only concatenated once
   - Environment variable paths like `/script-out/...` are relative to project root

3. **Missing diamond cut data**
   - Ensure both core and CTM upgrades are running
   - Verify `saveCombinedOutput()` is called after CTM upgrade completes
   - Check that CTM output file is being read correctly

### Debugging Failed Transactions with `cast run`

When you encounter "missing revert data" or unclear transaction failures, use this method to get the full execution
trace:

**Step 1: Extract transaction details from error**

From an error like:

```
transaction={ "data": "0xd52471c1...", "from": "0x97D2A9...", "to": "0xfe3EE966..." }
```

**Step 2: Send the transaction manually with sufficient gas**

```bash
TX_HASH=$(cast send <TO_ADDRESS> \
  "<CALLDATA>" \
  --value <VALUE_IF_NEEDED> \
  --private-key <PRIVATE_KEY> \
  --rpc-url http://127.0.0.1:8545 \
  --gas-price 50gwei \
  --gas-limit 10000000 2>&1 | grep "transactionHash" | awk '{print $2}')
```

**Important**: Use `--gas-limit 10000000` to ensure the transaction gets mined even if it reverts. This allows us to
trace it.

**Step 3: Trace the transaction to see where it failed**

```bash
cast run $TX_HASH --rpc-url http://127.0.0.1:8545
```

This will show the full call trace with:

- Every contract call in the execution path
- Function names and parameters
- Where exactly the revert occurred
- The revert reason (e.g., "call to non-contract address 0x0000...")

**Example**:

```bash
# From error message, extract: to=0xfe3EE966..., data=0xd52471c1..., value=1050000121535147500000

TX_HASH=$(cast send 0xfe3EE966E7790b427F7B078f304C7B4DDCd4bbfe \
  "0xd52471c10000000000000000000000000000000000000000000000000000000000000020..." \
  --value 1050000121535147500000 \
  --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 \
  --rpc-url http://127.0.0.1:8545 \
  --gas-price 50gwei \
  --gas-limit 10000000 2>&1 | grep "transactionHash" | awk '{print $2}')

cast run $TX_HASH --rpc-url http://127.0.0.1:8545
```

This will output:

```
Traces:
  [99306] Bridgehub::requestL2TransactionDirect(...)
    ├─ [92138] BridgehubImpl::requestL2TransactionDirect(...) [delegatecall]
    │   ├─ [70552] L1AssetRouter::bridgehubDepositBaseToken(...)
    │   │   ├─ [63432] L1AssetRouterImpl::bridgehubDepositBaseToken(...) [delegatecall]
    │   │   │   ├─ [48684] NativeTokenVault::bridgeBurn(...)
    │   │   │   │   └─ ← [Revert] call to non-contract address 0x0000000000000000000000000000000000000000
```

**Key benefits**:

- Shows the exact call path leading to the failure
- Reveals which contract call failed and why
- Makes it clear if a required contract is missing or uninitialized
- Much more informative than "missing revert data" errors

## General Rules

### NEVER USE try-catch OR staticcall in Upgrade Scripts

**THIS IS AN ABSOLUTE RULE - NO EXCEPTIONS**

❌ **FORBIDDEN PATTERNS:**

- `try contract.someFunction() { ... } catch { ... }`
- `(bool ok, bytes memory data) = target.staticcall(...)`

✅ **CORRECT APPROACH:**

- If a function reverts, fix the root cause (missing deployment, wrong order, etc.)
- Check if contracts exist before calling them: `if (address != address(0)) { ... }`
- Query protocol version or initialization state
- Restructure when the script runs

**WHY THIS RULE EXISTS:**

- try-catch and staticcall hide real errors instead of fixing them
- These patterns make debugging extremely difficult
- They mask initialization issues and timing problems
- The codebase should fail fast and clearly, not silently return defaults
