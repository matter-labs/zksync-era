# ShadowAccounts Design

## What Is a ShadowAccount

A ShadowAccount is a Smart Account deployed on a remote chain via interop, giving a user access management and contract interaction capabilities on that chain without the user directly deploying a smart account or having an EOA there. It is essentially a way to extend your on-chain presence to chains where you don't have a direct account.

Because a ShadowAccount is controlled by its owner on the home chain (via interop bundles), it can also act as an automated relay during bundle execution: an incoming bundle targets the ShadowAccount, which then performs follow-up actions (such as sending a new bundle to another chain, executing swaps, or interacting with protocols) in the same transaction — without requiring a separate user transaction on that chain.

### Primary Use Cases

1. **Private interop (origin-routed transfers):** Private tokens can only be transferred back to the origin chain, so a B→C transfer must go through the origin chain A as **B → A → C**. A ShadowAccount on chain A automates the A→C leg, so the user on B submits a single transaction and the two-hop routing is transparent. See [Private Interop Design](./private-interop.md) for details.

2. **L2→L1 interop:** A ShadowAccount deployed on L1 lets an L2 user interact with L1 contracts (governance, DeFi protocols, etc.) without needing a separate L1 account. See [L1 Interop Design](./l1-interop.md) for details.

3. **Private L1 interop (sender privacy):** A ShadowAccount on L1 owned by a shared `StealthSender` contract (rather than an individual user) enables L2 users to interact with L1 protocols without revealing their L2 address. See [Private L1 Interop Design](./private-l1-interop.md) for details.

## Deployment

ShadowAccounts are user contracts (not system contracts), deployed via a **ShadowAccountFactory** that uses CREATE2 with deterministic addresses so that the sender on a remote chain can predict the ShadowAccount address on the target chain without querying it.

### ShadowAccountFactory

The factory deploys ShadowAccounts with two inputs:
- **`_salt`** — a CREATE2 salt chosen by the deployer.
- **`_fullOwnerAddress`** — the ERC-7930 encoded owner address (chain ID + address).

```solidity
contract ShadowAccountFactory {
    /// @notice Deploys a ShadowAccount for the given owner.
    /// @param _salt CREATE2 salt chosen by the deployer.
    /// @param _fullOwnerAddress ERC-7930 encoded owner address.
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

The address is deterministic from `(factory address, salt, fullOwnerAddress)`. Given the same factory on multiple chains, the same salt and owner will produce the same address everywhere.

### Standard deployment (one per user per chain)

For the standard use case (non-private), a user deploys one ShadowAccount per chain they need presence on. The salt can be `bytes32(0)` or any fixed value — since the `fullOwnerAddress` already includes the user's chain ID and address, the combination is unique per user.

The InteropHandler can also deploy ShadowAccounts lazily (on first use) by calling the factory with a deterministic salt, so the user doesn't need to deploy manually.

### Privacy-preserving deployment

For [Private L1 Interop](./private-l1-interop.md), the ShadowAccount is owned by a shared `StealthSender` contract rather than an individual user. Multiple users share the same `fullOwnerAddress`, so the salt must be unique per user — typically a random value or an `ownerHash` derived from the user's address and a secret. See the Private L1 Interop doc for the full flow.

## Interface

The ShadowAccount needs to:
1. Be called by the InteropHandler via ERC-7786 `receiveMessage` to execute follow-up actions.
2. Execute arbitrary calls, including sending new bundles via InteropCenter (or PrivateInteropCenter).

### Execution Interface

The ShadowAccount implements ERC-7786's `receiveMessage` interface, so the InteropHandler delivers messages to it the same way as any other recipient. The message payload contains an array of calls to execute:

```solidity
enum ShadowAccountCallType {
    Call,
    DelegateCall
}

struct ShadowAccountCall {
    ShadowAccountCallType callType;
    address target;
    uint256 value;
    bytes data;
}

contract ShadowAccount is IERC7786Recipient {
    address public immutable INTEROP_HANDLER;
    bytes public fullOwnerAddress; // ERC-7930 encoded owner

    constructor(bytes memory _fullOwnerAddress) {
        INTEROP_HANDLER = msg.sender;
        fullOwnerAddress = _fullOwnerAddress;
    }

    function receiveMessage(
        bytes32 _receiveId,
        bytes calldata _sender,
        bytes calldata _payload
    ) external payable returns (bytes4) {
        require(msg.sender == INTEROP_HANDLER, "Only InteropHandler");
        require(keccak256(_sender) == keccak256(fullOwnerAddress), "Only owner");
        ShadowAccountCall[] memory calls = abi.decode(_payload, (ShadowAccountCall[]));
        for (uint256 i = 0; i < calls.length; ++i) {
            bool success;
            bytes memory returndata;
            if (calls[i].callType == ShadowAccountCallType.DelegateCall) {
                (success, returndata) = calls[i].target.delegatecall(calls[i].data);
            } else {
                (success, returndata) = calls[i].target.call{value: calls[i].value}(calls[i].data);
            }
            require(success, "Call failed");
        }
        return IERC7786Recipient.receiveMessage.selector;
    }

    receive() external payable {}
}
```

This is fully generic — the sender on the home chain encodes whatever calls they want (forwarding tokens, interacting with protocols, sending new bundles, etc.) into the payload.

### Open Questions

1. **Fee handling:** The second hop (A→C) requires gas and settlement fees. Who pays?
   - The original sender on B pre-pays fees for both hops?
   - The ShadowAccount holds a fee balance that gets topped up?
   - The bundle from B includes value/fee allocation for the second hop?

## Authorization

The ShadowAccount stores the owner address at deployment. Authorization is enforced with two checks in `receiveMessage`:

1. **`msg.sender` must be the InteropHandler** — ensures the call comes from a verified interop bundle, not an arbitrary contract.
2. **`_sender` (the ERC-7786 message sender) must match the stored owner** — ensures only the owner on the home chain can control this ShadowAccount.

This means only bundles originating from the owner's address on the source chain can execute calls through the ShadowAccount. No other user can hijack it, and malicious bundles from other senders are rejected.

The owner can be:
- **An individual user's address** (standard use case) — the ShadowAccount is linked to a specific user.
- **A shared contract like `StealthSender`** (privacy use case) — the ShadowAccount trusts the shared contract to verify individual user ownership internally. See [Private L1 Interop](./private-l1-interop.md).

## Reading Remote State

When a ShadowAccount executes on a remote chain (e.g., L1), the bundle calls may need to reference state that only exists on that chain — for example, reading a swap price from a DEX, querying an oracle, or checking a protocol's current parameters. The problem is that the bundle is constructed on the sender's home chain (L2), where this remote state is not available.

### Solution: Delegatecall to Script Contracts

The ShadowAccount supports `delegatecall` as a call type (see `ShadowAccountCallType` above). The bundle can include a delegatecall to a pre-deployed **script contract** that contains arbitrary Solidity logic. The script runs in the ShadowAccount's context — it can read any remote state, make calls on behalf of the ShadowAccount, and execute complex multi-step logic.

```
ShadowAccount
  ├─ Call 1 (call): transfer tokens to ShadowAccount
  └─ Call 2 (delegatecall): SwapScript
       ├─ reads price from DEX
       ├─ calculates optimal swap parameters
       ├─ executes swap
       └─ bridges result back to L2
```

Script contracts are standard Solidity contracts that can be pre-deployed and reused. For example, a "swap with slippage protection" script, an "Aave deposit + borrow" script, etc. The user or SDK deploys the script once, and any bundle can delegatecall it.

This approach is fully expressive — any logic that can be written in Solidity can be encoded in a script contract — without introducing custom encoding or a mini-VM inside the ShadowAccount.
