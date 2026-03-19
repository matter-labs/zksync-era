# Private L1 Interop Design

## Problem Statement

[L1 Interop](./l1-interop.md) allows L2 users to send interop bundles that execute on L1, using
[ShadowAccounts](./shadow-accounts.md) as their on-chain identity. However, the standard ShadowAccount's address is
deterministically derived from `(ownerChainId, ownerAddress)` — anyone on L1 can compute the address and link it back to
the user's L2 identity. This means all L1 activity (DeFi interactions, governance votes, token movements) is publicly
attributable to a specific L2 address.

We want L2 users to be able to interact with L1 protocols via interop **without revealing their L2 address on L1**.

## Design Goals

1. **Sender privacy on L1** — L1 observers cannot link the executing ShadowAccount to a specific L2 user address.
2. **Reuse existing ShadowAccount contract** — No new account implementation needed. The standard `ShadowAccount`
   already supports arbitrary owner addresses. Each user deploys their own ShadowAccount (with a random CREATE2 salt for
   uniqueness), but all share the same owner = `StealthSender`. The salt is random, not derived from the user's address,
   so the deployed address is not linkable to any specific user.
3. **Same interop interfaces** — Uses `PrivateInteropCenter` and `L1InteropHandler` as designed. StealthSender
   integrates as an `IL2CrossChainSender` (indirect call target), the same pattern used by `L2AssetRouter` for token
   bridging. SDK changes are minimal.
4. **Single-bundle flow** — Token bridging and ShadowAccount commands are combined in a single bundle. Both use indirect
   calls, so neither reveals the user's address in the `from` field.
5. **Works for L2↔L2 too** — The same pattern applies when the sender wants to hide their identity on the destination L2
   chain.

## Key Mechanism: Indirect Calls Hide the Sender

In `InteropCenter._processCallStarter()`, indirect calls set the `from` field to the **recipient contract** (the
`IL2CrossChainSender`), not the original caller:

```solidity
// InteropCenter._processCallStarter() (simplified)
if (_callStarter.callAttributes.indirectCall) {
    InteropCallStarter memory actual = IL2CrossChainSender(recipientAddress)
        .initiateIndirectCall(_destinationChainId, _sender, _value, _data);
    interopCall.from = recipientAddress;  // ← NOT the user
} else {
    interopCall.from = _sender;           // ← the user (msg.sender)
}
```

This is already how token bridging works: the user makes an indirect call to `L2AssetRouter`, so `from = L2AssetRouter`
in the InteropCall. The user's address is passed as `_originalCaller` to `initiateIndirectCall` internally but never
appears in the bundle's InteropCall.

StealthSender uses the exact same pattern: it implements `IL2CrossChainSender`, so an indirect call to StealthSender
produces `from = StealthSender` — hiding the user.

## Architecture Overview

```
L2 Chain B                                    L1
──────────                                    ──

User calls PrivateInteropCenter.sendBundle    ShadowAccount S1 (user Alice)
  with two indirect calls:                      ├─ owner = (chainB, StealthSender)
                                                ├─ deployed with random salt₁
  call 1 (indirect, to=L2AssetRouter):          └─ executes calls on L1
    → bridges tokens to S on L1               ShadowAccount S2 (user Bob)
    → from = L2AssetRouter (user hidden)        ├─ owner = (chainB, StealthSender)
                                                ├─ deployed with random salt₂
  call 2 (indirect, to=StealthSender):          └─ executes calls on L1
    → forwards message to S on L1             ...
    → from = StealthSender (user hidden)
```

Both calls in the bundle use indirect calls, so neither `from` field reveals the user. The user's address is only known
to the source chain contracts (L2AssetRouter and StealthSender) during processing, but never appears in the InteropCall
data that reaches L1.

## New Contracts

### 1. StealthSender (deployed on L2)

A shared contract that implements `IL2CrossChainSender`. When used as an indirect call target in a bundle, it produces
an InteropCall with `from = StealthSender` (not the user).

Multiple users send through the same StealthSender. Each user registers with a secret; the StealthSender computes
`ownerHash = hash(userAddress, secret)`, which serves as both the ownership proof and the CREATE2 salt for the L1
ShadowAccount.

```solidity
contract StealthSender is IL2CrossChainSender {
    ShadowAccountFactory public immutable L1_SHADOW_ACCOUNT_FACTORY;
    uint256 public immutable L1_CHAIN_ID;

    /// @dev Maps user address => secret.
    /// Stored so the user only needs to provide it once during registration.
    mapping(address user => bytes32 secret) public secrets;

    /// @dev Maps ownerHash => user address.
    /// Used by receiveReturn() to look up who to forward tokens to.
    mapping(bytes32 ownerHash => address user) public ownerHashToUser;

    /// @notice Register a secret for the user's ShadowAccount on L1.
    /// The ownerHash = keccak256(userAddress, secret) is used as the CREATE2
    /// salt when deploying the ShadowAccount on L1 via ShadowAccountFactory.
    /// @param _secret Random value chosen by the user.
    function register(bytes32 _secret) external {
        secrets[msg.sender] = _secret;
        bytes32 ownerHash = keccak256(abi.encodePacked(msg.sender, _secret));
        ownerHashToUser[ownerHash] = msg.sender;
    }

    /// @notice Compute the ownerHash that serves as the CREATE2 salt.
    function getOwnerHash(address _user) public view returns (bytes32) {
        bytes32 secret = secrets[_user];
        require(secret != bytes32(0), "Not registered");
        return keccak256(abi.encodePacked(_user, secret));
    }

    /// @notice Called when tokens are returned from L1 via a private interop bundle.
    /// Looks up the user from the ownerHash and forwards all received tokens.
    /// @param _ownerHash The ownerHash identifying which user to forward to.
    /// @param _token The token address to forward (address(0) for ETH).
    /// @param _amount The amount to forward.
    function receiveReturn(bytes32 _ownerHash, address _token, uint256 _amount) external {
        address user = ownerHashToUser[_ownerHash];
        require(user != address(0), "Unknown ownerHash");
        // Transfer tokens to the user
        if (_token == address(0)) {
            (bool success,) = user.call{value: _amount}("");
            require(success);
        } else {
            IERC20(_token).transfer(user, _amount);
        }
    }

    /// @notice Compute the L1 ShadowAccount address for a user.
    function getShadowAccountAddress(
        address _user
    ) external view returns (address) {
        bytes32 salt = getOwnerHash(_user);
        bytes memory fullOwnerAddress = InteroperableAddress.formatEvmV1(
            block.chainid, address(this)
        );
        return L1_SHADOW_ACCOUNT_FACTORY.computeAddress(
            salt, fullOwnerAddress
        );
    }

    /// @notice Called by InteropCenter when processing an indirect call.
    /// Verifies the caller owns a stealth account and returns an
    /// InteropCallStarter targeting their ShadowAccount on L1.
    ///
    /// Because this is an indirect call, the resulting InteropCall will have
    /// from = address(this) (StealthSender), NOT _originalCaller.
    function initiateIndirectCall(
        uint256 _destinationChainId,
        address _originalCaller,
        uint256 _value,
        bytes calldata _data
    ) external payable onlyInteropCenter returns (InteropCallStarter memory) {
        require(_destinationChainId == L1_CHAIN_ID, "Only L1 destination");

        // Compute the user's ShadowAccount address on L1
        bytes32 salt = getOwnerHash(_originalCaller);
        bytes memory fullOwnerAddress = InteroperableAddress.formatEvmV1(
            block.chainid, address(this)
        );
        address stealthAccount = L1_SHADOW_ACCOUNT_FACTORY.computeAddress(
            salt, fullOwnerAddress
        );

        // Return the InteropCallStarter targeting the ShadowAccount.
        // _data contains the ShadowAccountCall[] payload that S will execute.
        return InteropCallStarter({
            to: InteroperableAddress.formatEvmV1(_destinationChainId, stealthAccount),
            data: _data,
            callAttributes: new bytes[](0)
        });
    }
}
```

### 2. ShadowAccount on L1 (standard contract, no changes)

Each user deploys their own `ShadowAccount` on L1 via the `ShadowAccountFactory`, all with the same owner:

```solidity
fullOwnerAddress = formatEvmV1(chainB, StealthSender)
```

Each deployment uses a different random CREATE2 salt, giving each user a unique address. But all these accounts share
the same `fullOwnerAddress` — they all trust the same `StealthSender` contract as their owner.

The ShadowAccount's `receiveMessage` checks that `_sender` matches this owner. Since the bundle's `from` is the
`StealthSender` contract (set by the indirect call mechanism), this check passes. The ShadowAccount trusts that
`StealthSender` already verified the caller's ownership on chain B.

No modifications to the ShadowAccount contract are needed.

### 3. ShadowAccount Factory (on L1)

A factory contract that deploys ShadowAccounts with arbitrary owners via CREATE2, allowing users to pre-compute their
account addresses.

```solidity
contract ShadowAccountFactory {
    /// @notice Deploys a ShadowAccount for the given owner.
    /// @param _salt CREATE2 salt chosen by the user.
    /// @param _fullOwnerAddress ERC-7930 encoded owner (e.g., the StealthSender on chain B).
    function deploy(
        bytes32 _salt,
        bytes memory _fullOwnerAddress
    ) external returns (address) {
        ShadowAccount account = new ShadowAccount{salt: _salt}(_fullOwnerAddress);
        return address(account);
    }

    /// @notice Computes the address where a ShadowAccount would be deployed.
    function computeAddress(
        bytes32 _salt,
        bytes memory _fullOwnerAddress
    ) external view returns (address) {
        bytes memory bytecode = abi.encodePacked(
            type(ShadowAccount).creationCode,
            abi.encode(_fullOwnerAddress)
        );
        return address(uint160(uint256(keccak256(
            abi.encodePacked(bytes1(0xff), address(this), _salt, keccak256(bytecode))
        ))));
    }
}
```

The user chooses a random `_salt` when deploying. Since the salt is not derived from the user's address, the deployed
ShadowAccount address is not linkable to any specific L2 user.

## Full Flow

### Setup (once per user)

1. **User registers on L2:**
   - Picks a random `secret`.
   - Calls `StealthSender.register(secret)` on chain B.
   - The contract stores the secret and can compute `ownerHash = hash(userAddress, secret)`.
   - The user can call `getShadowAccountAddress(userAddress)` to get their L1 address.

2. **User deploys ShadowAccount on L1:**
   - Calls `ShadowAccountFactory.deploy(ownerHash, formatEvmV1(chainB, StealthSender))` on L1, using the `ownerHash` as
     the CREATE2 salt.
   - This can be done by anyone (permissionless deployment) — the user can ask a relayer to deploy it, deploy it
     themselves via a standard L1 transaction, or include the factory deploy call in an L2→L1 interop bundle (the first
     bundle targeting L1 can deploy the ShadowAccount and use it in the same atomic execution).

### Usage (each interaction)

The user sends a **single bundle** via `PrivateInteropCenter` with two indirect calls:

```
L2 Chain B                                       L1
──────────                                       ──

1. User calls PrivateInteropCenter.sendBundle(
     dest = L1,
     calls = [
       { indirect, to: L2AssetRouter,            (token bridging)
         data: bridge ETH to S },
       { indirect, to: StealthSender,            (message forwarding)
         data: [deposit to Aave, borrow GHO,
                approve, bridge back] }
     ]
   )

2. InteropCenter processes the bundle:
   ├─ Call 1: L2AssetRouter.initiateIndirectCall()
   │   → burns tokens from user
   │   → InteropCall.from = L2AssetRouter        (user hidden ✓)
   └─ Call 2: StealthSender.initiateIndirectCall()
       → verifies user, computes ShadowAccount S
       → InteropCall.from = StealthSender         (user hidden ✓)
       → InteropCall.to = S

3. PrivateInteropCenter sends L2→L1 log:
   bundle HASH only (private interop)

                                                 4. After L2 batch settles:

                                                 5. Keeper calls L1InteropHandler
                                                    ├─ Verifies L2 message proof
                                                    └─ Executes bundle:
                                                       ├─ Call 1: finalizeDeposit
                                                       │   → ETH arrives at S
                                                       └─ Call 2: S.receiveMessage(
                                                            sender=(chainB, StealthSender) ✓
                                                            payload=[Aave calls])

                                                 6. ShadowAccount S executes atomically:
                                                    ├─ Deposit ETH into Aave
                                                    ├─ Borrow GHO
                                                    └─ GHO stays in S (for now)
```

Both calls in the bundle have `from` set to their respective indirect call targets (L2AssetRouter and StealthSender),
not the user.

### Required Change: Private Bridge Encoding

While indirect calls hide the user from the `InteropCall.from` field, the standard bridge encoding **leaks the user's
address in the call payload**. In `NativeTokenVaultBase.bridgeBurn()`, the `_originalCaller` (the user) is encoded into
`bridgeMintData`:

```solidity
// NativeTokenVaultBase._bridgeBurnNativeToken() — current behavior
_bridgeMintData = DataEncoding.encodeBridgeMintData({
    _originalCaller: _originalCaller,  // ← user's address in payload!
    _remoteReceiver: _receiver,
    _originToken: _nativeToken,
    _amount: _depositAmount,
    _erc20Metadata: erc20Metadata
});
```

This `bridgeMintData` becomes the `InteropCall.data` field in the bundle. So even though
`InteropCall.from = L2AssetRouter`, the actual payload contains the user's address — visible to anyone who sees the
executed bundle on L1.

Additionally, the `BridgeBurn` event on chain B emits `sender: _originalCaller`, creating a linkable on-chain event.

**The fix:** For private interop transfers, the bridge encoding must replace `_originalCaller` with a neutral value
(e.g., `address(0)` or the AssetRouter address). This requires a change in how `L2AssetRouter.initiateIndirectCall()`
passes the caller to the NTV when the transfer originates from a `PrivateInteropCenter`:

```solidity
// L2AssetRouter.initiateIndirectCall() — modified for private interop
function initiateIndirectCall(
    uint256 _chainId,
    address _originalCaller,
    uint256 _value,
    bytes calldata _data
) external payable onlyL2InteropCenter returns (InteropCallStarter memory) {
    // For private interop, replace the original caller with a neutral address
    // to prevent leaking the user's identity in the bridge mint data.
    address callerForBridge = _isPrivateInteropCenter(msg.sender)
        ? address(this)  // L2AssetRouter as the "caller"
        : _originalCaller;

    L2TransactionRequestTwoBridgesInner memory request = _bridgehubDeposit({
        _chainId: _chainId,
        _originalCaller: callerForBridge,
        _value: _value,
        _data: _data,
        _nativeTokenVault: ntvAddr
    });
    // ...
}
```

This ensures that for private transfers:

- `InteropCall.from = L2AssetRouter` (from indirect call mechanism)
- `InteropCall.data` contains `_originalCaller = L2AssetRouter` (from modified encoding)
- `BridgeBurn` event emits `sender = L2AssetRouter` (no user leak)

The `_originalCaller` is still needed for the actual token burn (`transferFrom`), so the NTV must pull tokens from the
real user while encoding a neutral address in the mint data. This can be achieved by passing both the real caller (for
token operations) and the encoded caller (for mint data) separately.

### Private Return Path (L1 → L2)

After the ShadowAccount has completed its L1 operations (e.g., borrowed GHO from Aave), the tokens need to return to the
user on L2 **without revealing the user's L2 address**.

**The problem:** If the ShadowAccount uses standard L1→L2 bridging (`Bridgehub.requestL2TransactionTwoBridges()`), the
recipient address is visible in the L1 transaction and on L2. This leaks the user's identity, undoing the privacy gained
on the outbound path.

**The solution:** The ShadowAccount sends an **L1→L2 interop bundle** (via an `L1PrivateInteropCenter`) with:

1. An indirect call to bridge tokens to StealthSender on L2
2. A direct call to StealthSender on L2, passing the `ownerHash` so it knows which user to forward to

```
L1                                               L2 Chain B
──                                               ──────────

7. ShadowAccount S (via delegatecall to script):
   └─ L1PrivateInteropCenter.sendBundle(
        dest = Chain B,
        calls = [
          { indirect, to: L1AssetRouter,         (token bridging)
            data: bridge GHO to StealthSender
                  on chain B },
          { direct, to: StealthSender,           (withdrawal notification)
            data: receiveReturn(ownerHash) }
        ]
      )

   L1PrivateInteropCenter sends L1→GW log:
   bundle HASH only

                                                 8. Keeper calls PrivateInteropHandler
                                                    on chain B, executes bundle:
                                                    ├─ Call 1: finalizeDeposit
                                                    │   → GHO arrives at StealthSender
                                                    └─ Call 2: StealthSender.receiveReturn
                                                         → verifies ownerHash is registered
                                                         → looks up user from ownerHash
                                                         → forwards GHO to user
```

The key elements:

- **L1AssetRouter** bridges tokens to `StealthSender` on L2 (not to the user directly). `from = L1AssetRouter`, user not
  revealed.
- **Direct call to StealthSender** passes the `ownerHash`. The `from` field is the ShadowAccount address S on L1 — this
  is fine because S is not linkable to any specific L2 user (that's the whole point of the outbound privacy).
  StealthSender doesn't check `from` for returns; it verifies via `ownerHash`.
- **StealthSender on L2** receives the tokens and the `ownerHash`, verifies the `ownerHash` corresponds to a registered
  user, and transfers the tokens to that user.
- The entire return bundle goes through `L1PrivateInteropCenter`, so only the bundle hash is published.

This requires:

- An `L1PrivateInteropCenter` for sending private bundles from L1 to L2 — answering
  [L1 Interop open question #3](./l1-interop.md) in the affirmative.
- StealthSender to have a `receiveReturn(bytes32 ownerHash)` function that accepts tokens + ownerHash and forwards to
  the registered user. This needs a reverse lookup `ownerHash → user address`, which StealthSender can maintain
  alongside the existing `secrets` mapping.

## Privacy Properties

### What each observer sees

| Observer    | Visible information                                                                                                                   | Hidden information                                       |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| **Gateway** | Bundle hash + call count (private interop)                                                                                            | Bundle contents, sender, recipient, amounts              |
| **L1**      | `from` = L2AssetRouter and StealthSender (shared contracts). ShadowAccount S executes DeFi calls.                                     | Which L2 user owns S                                     |
| **Chain B** | User called `PrivateInteropCenter.sendBundle()` with indirect calls to L2AssetRouter and StealthSender. `register()` stored a secret. | The L1 destination address S (inside the private bundle) |

### Linkability analysis

- **L1 → L2 user:** L1 observers see ShadowAccount S with owner = `(chainB, StealthSender)`. StealthSender is a shared
  contract used by many users. The CREATE2 salt used to deploy S is an `ownerHash` that cannot be reversed without
  knowing both the user's address and secret. No link to any specific L2 user.
- **Chain B `register()` call → L1 account:** The registration stores a secret keyed by user address. An observer can
  see that a user registered, but the `ownerHash` (which is the CREATE2 salt) is not stored directly — it must be
  computed, and the secret alone doesn't reveal which L1 address it maps to without knowing the factory address and
  owner encoding.
- **Chain B `sendBundle()` call → L1 account:** The user calls `PrivateInteropCenter.sendBundle()` with indirect calls.
  The ShadowAccount address is computed inside `StealthSender.initiateIndirectCall()` and embedded in the InteropCall —
  but the full bundle goes through PrivateInteropCenter, so only the bundle hash is published on-chain.

### Threat model limitations

- **Keeper/relayer:** The entity that executes the bundle on L1 sees the full bundle data (they must, to submit it).
  They know which ShadowAccount is being called and what it does. If the keeper is trusted or if the user runs their own
  keeper, this is not a concern. A decentralized keeper network would require additional privacy measures.
- **Timing correlation:** If a user registers on chain B and a ShadowAccount is deployed on L1 shortly after, a
  motivated observer could correlate the two events. Mitigation: batch registrations, delayed deployment, or deploying
  the L1 account well in advance.

## Application to L2↔L2 Private Sender Privacy

The same pattern works for hiding the sender's identity on a destination L2 chain. The user sends a single bundle via
`PrivateInteropCenter` with:

- An indirect call to `L2AssetRouter` to bridge tokens (from = L2AssetRouter)
- An indirect call to `StealthSender` to forward commands to a ShadowAccount on chain C (from = StealthSender)

```
L2 Chain B (sender)                   L2 Chain C (destination)
───────────────────                   ───────────────────────

PrivateInteropCenter.sendBundle([     ShadowAccount (S)
  indirect → L2AssetRouter,             ├─ owner = (chainB, StealthSender)
  indirect → StealthSender              └─ PrivateInteropHandler executes:
])                                         ├─ finalizeDeposit (tokens arrive)
  └─ bundle hash to GW                    └─ S.receiveMessage(
                                                sender=(chainB, StealthSender)
                                                payload=[forward to recipient])
```

Chain C observers see `from = StealthSender on chain B` and `from = L2AssetRouter` — they cannot determine which user on
chain B initiated the transfer.

## Relation to Other Designs

- **[L1 Interop](./l1-interop.md):** Private L1 interop builds on L1 interop. The `L1InteropHandler` is the same; the
  difference is the sender-side privacy via indirect calls through `StealthSender` + private interop.
- **[Private Interop](./private-interop.md):** Uses `PrivateInteropCenter` to hide bundle data on the Gateway. Private
  L1 interop combines this with `StealthSender` to also hide the sender's identity on the destination chain.
- **[ShadowAccounts](./shadow-accounts.md):** The L1 ShadowAccount is a standard `ShadowAccount` instance. No contract
  changes needed. The only difference is that the owner is set to a shared contract (`StealthSender`) rather than an
  individual user EOA.
- **[Prividium Addresses](https://github.com/mm-zk-codex/prividium-addresses):** External PoC that implements stealth
  address forwarding for L1↔L2 bridging. The `StealthSender` + `ShadowAccount` approach achieves similar privacy
  properties but integrates natively with the interop framework — reusing `ShadowAccount`, `PrivateInteropCenter`,
  `IL2CrossChainSender`, and `L1InteropHandler` — rather than requiring separate forwarder contracts and relayer
  infrastructure.

## Contract Summary

| Contract                 | Chain      | New or Existing         | Purpose                                                                                                                                |
| ------------------------ | ---------- | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `StealthSender`          | L2         | **New**                 | `IL2CrossChainSender` that hides users behind a shared `from` via indirect calls. Also receives returned tokens and forwards to users. |
| `ShadowAccountFactory`   | L1 (or L2) | **New**                 | Deploys ShadowAccounts with arbitrary owners via CREATE2                                                                               |
| `L1PrivateInteropCenter` | L1         | **New**                 | Sends private bundles from L1 to L2 (for return path)                                                                                  |
| `ShadowAccount`          | L1 (or L2) | Existing (no changes)   | Executes calls on behalf of its owner                                                                                                  |
| `L2AssetRouter`          | L2         | Existing (**modified**) | `initiateIndirectCall` must replace `_originalCaller` with neutral address for private interop to prevent payload leak                 |
| `PrivateInteropCenter`   | L2         | Existing (no changes)   | Sends bundles with hash-only publication                                                                                               |
| `L1InteropHandler`       | L1         | Existing (no changes)   | Verifies and executes L2→L1 bundles                                                                                                    |

## Open Questions

1. **L1→L2 interop bundles (required for private return path):** The return path requires an `L1PrivateInteropCenter`
   that can send bundles from L1 to L2. This answers [L1 Interop open question #3](./l1-interop.md) — yes, L1→L2 bundles
   are needed. The design of `L1InteropCenter` / `L1PrivateInteropCenter` needs to be specified.

2. **ownerHash → user mapping privacy:** The `ownerHashToUser` mapping is stored in StealthSender and is enumerable
   on-chain. An observer can see all registered `ownerHash → user` pairs. However, linking an `ownerHash` to a specific
   L1 ShadowAccount still requires knowing the factory address and computing the CREATE2 address — the `ownerHash` alone
   doesn't reveal which L1 account it corresponds to without the full computation. This is acceptable for the current
   threat model but could be strengthened with a commitment scheme if needed.

3. **Keeper privacy:** The keeper/relayer sees full bundle data. Should we explore a commit-reveal scheme or encrypted
   mempools for keeper submissions?

4. **Registration privacy:** The `register()` call on chain B is a transaction from the user's address. Even though the
   stored data is only a secret, the transaction itself is observable. Should `register()` accept meta-transactions or
   use a relayer to hide the caller?

5. **ShadowAccount deployment on L1:** Who deploys the ShadowAccount? The user would need an L1 transaction (or use a
   CREATE2 deployer relayer). Alternatively, the `L1InteropHandler` could deploy ShadowAccounts lazily (like the L2
   InteropHandler does), but this would require L1InteropHandler to know about the `ShadowAccountFactory`.

6. **Multiple StealthSenders:** Should there be one `StealthSender` per L2 chain, or could multiple exist (e.g.,
   different privacy policies, different supported destination chains)? The ShadowAccount on L1 only accepts messages
   from one owner — if the user wants to use a different StealthSender, they need a different ShadowAccount.

7. **Token bridging destination leak:** The indirect call to L2AssetRouter bridges tokens to the ShadowAccount address S
   on L1. The user specifies S as the recipient in the bridge call data. Since this data is inside the private bundle
   (hash only published), S is not revealed on chain B or GW — but the L2AssetRouter on chain B processes it internally.
   Need to confirm that no event or storage on chain B leaks the recipient address.
