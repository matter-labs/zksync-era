# Intro Guide to Interop

## What is Interop

Interop is ZKsync's cross-chain communication system that enables seamless interaction between different L2 chains in the ecosystem. It implements the **[ERC-7786](https://eips.ethereum.org/EIPS/eip-7786)** standard for cross-chain messaging and **[ERC-7930](https://eips.ethereum.org/EIPS/eip-7930)** for interoperable addresses, ensuring alignment with the broader Ethereum ecosystem.

The system provides three main capabilities:

1. **Broadcast Messages**: Send messages from any L2 that can be verified on any other chain (low-level primitive)
2. **Cross-Chain Calls**: Execute a single function on another L2 with value transfer and permission controls  
3. **Cross-Chain Bundles**: Execute multiple calls atomically on another L2 - all succeed or all fail together

Key benefits include secure asset transfers, flexible permission controls, atomic multi-step operations, and cryptographic proof systems for cross-chain verification.

> **Note**: For simplified workflows, we provide the **[InteropLibrary](./interop_library.md)** helper that wraps common interop operations like token transfers, native sends, and direct calls with convenient functions. Throughout this guide, we'll show both the raw interop approach and the library shortcuts.

## How to Use Interop

### Common Use Cases

#### Cross-Chain ERC20 Transfer

One of the most common interop use cases is transferring tokens between chains. You can use the InteropLibrary helper for simplicity:

```solidity
// Simple token transfer using InteropLibrary
import {InteropLibrary} from "deploy-scripts/InteropLibrary.sol";

bytes32 bundleHash = InteropLibrary.sendToken(
    destinationChainId,  // Chain ID where tokens should go
    l2TokenAddress,      // Token contract on current L2
    amount,              // Amount to transfer (with decimals)
    recipient,           // Who receives on destination chain
    unbundlerAddress     // Who can execute on destination (or address(0) for sender)
);
```

Or if you need more control, here's the manual approach:

```solidity
// Manual token transfer for advanced use cases
address l2TokenAddress = 0x...; // Your token on current L2
uint256 amount = 100 * 10**18;  // Amount to transfer

// Get the asset ID for the token
bytes32 assetId = L2NativeTokenVault.assetId(l2TokenAddress);

// Prepare the transfer data using DataEncoding helper
bytes memory transferData = DataEncoding.encodeAssetRouterBridgehubDepositData(
    assetId,
    DataEncoding.encodeBridgeBurnData(amount, recipient, address(0))
);

// Create the call starter with indirect call attribute
InteropCallStarter[] memory calls = new InteropCallStarter[](1);
calls[0] = InteropCallStarter({
    to: InteroperableAddress.formatEvmV1(L2_ASSET_ROUTER_ADDR), // No chain ID here!
    data: transferData,
    callAttributes: new bytes[](1)
});
calls[0].callAttributes[0] = abi.encodeCall(IERC7786Attributes.indirectCall, (0));

// Set bundle attributes (e.g., unbundler)
bytes[] memory bundleAttributes = new bytes[](1);
bundleAttributes[0] = abi.encodeCall(
    IERC7786Attributes.unbundlerAddress,
    InteroperableAddress.formatEvmV1(unbundlerAddress)
);

// Send the bundle
bytes32 bundleHash = InteropCenter.sendBundle(
    InteroperableAddress.formatEvmV1(destinationChainId), // Destination chain here
    calls,
    bundleAttributes
);
```

The AssetRouter handles locking tokens on the source chain and minting/releasing them on the destination.

### Broadcasting Messages (Low-Level)

For simple data broadcasting that any chain can verify:

```solidity
// Send a broadcast message via L1Messenger
bytes memory message = abi.encode("Hello from Chain A!", block.timestamp);
bytes32 messageHash = L1Messenger.sendToL1(message);

// On another chain, anyone can verify this message was sent and received
L2MessageVerification.proveL2MessageInclusionShared(
    sourceChainId,
    batchNumber,
    messageIndex,
    message,
    proof
);
```

This is useful for cross-chain proofs, attestations, or any scenario where you need to prove something happened on another chain.

> **InteropLibrary Alternative**: For L1 message sending:
> ```solidity
> bytes32 messageHash = InteropLibrary.sendMessage(messageData);
> ```

### Cross-Chain Calls and ERC-7786

For executing specific functions on other L2s, we implement the **ERC-7786** standard. The naming convention follows this standard - hence `sendMessage` for sending calls and `receiveMessage` for receiving them.

#### Single Cross-Chain Call

Perfect for simple operations like calling a function on another chain:

```solidity
// Send a single cross-chain call to another L2 (ERC-7786 sendMessage)
bytes32 sendId = InteropCenter.sendMessage{value: 0.1 ether}(
    recipient,      // ERC-7930 address (chain ID + address)
    payload,        // Your encoded function call
    attributes      // Optional attributes (permissions, value, etc.). We expand on attributes usage below.
);
```

> **InteropLibrary Alternative**: For direct calls with execution/unbundler control, use:
> ```solidity
> bytes32 sendId = InteropLibrary.sendDirectCall(
>     destinationChainId, target, data, executionAddress, unbundlerAddress
> );
> ```

Under the hood, this creates a bundle with just one call - but you don't need to worry about that complexity.
On the destination chain, your target contract must implement the **ERC-7786** `receiveMessage` function:

```solidity
contract MyContract is IERC7786Recipient {
    function receiveMessage(
        bytes32 receiveId,
        bytes calldata sender,    // ERC-7930 address of sender
        bytes calldata payload
    ) external payable returns (bytes4) {
        // Your logic here
        return IERC7786Recipient.receiveMessage.selector;
    }
}
```

### Bundles and Call Starters

#### Understanding Call Starters

A **call starter** is the building block for bundling multiple cross-chain calls together. You only need call starters when using `sendBundle` to combine multiple calls - for single calls, we can just use `sendMessage` directly.

Each call starter contains:
- `to`: The recipient (in ERC-7930 format - like an email address with chain ID)
- `data`: The function call to execute
- `callAttributes`: Optional settings like value or permissions

##### ERC-7930 Address Format

> **Note**: You can use the [OpenZeppelin InteroperableAddress library](https://docs.openzeppelin.com/contracts/5.x/api/utils#interoperableaddress) which provides functionality for working with ERC-7930 addresses.

The ERC-7930 address format encodes both the chain ID and address in a single bytes value. Here's how it works:

```solidity
// ERC-7930 address structure:
// [1 byte: version] [32 bytes: chain ID] [1 byte: address length] [20 bytes: address]

// Example: Encoding zkSync Era mainnet (chain 324) + address 0x1234...5678
bytes memory interopAddress = InteroperableAddress.formatEvmV1(
    324,                                    // Chain ID
    0x1234567890123456789012345678901234567890  // Address
);
// Result: 0x01000000000000000000000000000000000000000000000000000000000000014414[20 bytes of address]
//         ^^ version 1    ^^ chain ID 324 (hex: 0x144)                      ^^ length 20 bytes
```

Example usage in a call starter:
```solidity
InteropCallStarter[] memory calls = new InteropCallStarter[](1);
calls[0] = InteropCallStarter({
    to: InteroperableAddress.formatEvmV1(324, recipient), // Creates ERC-7930 address
    data: abi.encodeCall(IContract.updateValue, (42)),
    callAttributes: new bytes[](0)
});
```

#### Bundled Cross-Chain Calls

For complex operations requiring multiple calls to execute atomically:

```solidity
// Send multiple calls as an atomic bundle
InteropCallStarter[] memory calls = new InteropCallStarter[](3); // We expand on Call Starters below.
// ... populate calls ...

bytes32 bundleHash = InteropCenter.sendBundle{value: totalValue}(
    destinationChain,
    calls,
    bundleAttributes
);
```

> **InteropLibrary Alternative**: For bundles of direct calls:
> ```solidity
> bytes32 bundleHash = InteropLibrary.sendDirectCallBundle(
>     destinationChainId, targets, dataArray, executionAddress, unbundlerAddress
> );
> ```

The key difference: bundles can ensure all calls execute together (all-or-nothing), while single calls are simpler for basic needs.

#### Unbundling Functionality

The unbundling functionality gives you fine-grained control over bundle execution. If you're the unbundler, you can control bundle execution. 

**Important**: When no unbundler is specified, the **default unbundler** is the sender's full ERC-7930 address (chain ID + address). This means only the original sender calling from the same chain can unbundle - not just any address matching the sender's address on other chains.

As the unbundler, you can:

- **Execute a subset of calls**: Choose which calls to execute and leave others unprocessed
- **Cancel a subset of calls**: Mark specific calls as cancelled so they can never be executed
- **Mix both**: Execute some calls and cancel others in the same unbundling operation

```solidity
// Example: In a 4-call bundle, execute calls 0 and 2, cancel call 1, skip call 3
CallStatus[] memory desiredStatus = new CallStatus[](4);
desiredStatus[0] = CallStatus.Executed;    // Execute first call
desiredStatus[1] = CallStatus.Cancelled;   // Cancel second call  
desiredStatus[2] = CallStatus.Executed;    // Execute third call
desiredStatus[3] = CallStatus.Unprocessed; // Skip fourth call (leave for later)

InteropHandler.unbundleBundle(sourceChainId, bundleData, desiredStatus);
```

**Key points about unbundling**:
- You can call `unbundleBundle` multiple times until all calls are processed
- Once a call is executed or cancelled, its status is permanent
- Only the designated unbundler (or sender if none specified) can unbundle
- Single calls sent via `sendMessage` can also be unbundled since they're 1-call bundles

### Message Value Considerations

When sending cross-chain calls with value, the `msg.value` you send gets handled differently based on the destination chain's base token. If both chains share the same base token, the value is burned on the source and minted on the destination. If they have different base tokens, the system routes through the appropriate token bridging mechanism.

> **InteropLibrary Alternative**: For native token transfers, simply use:
> ```solidity
> bytes32 bundleHash = InteropLibrary.sendNative(
>     destinationChainId, recipient, unbundlerAddress, amount
> );
> ```

### Custom Attributes

ERC-7786 allows custom attributes to control call behavior. ZKsync supports these attributes:

> **InteropLibrary Helpers**: The library provides `buildCallAttributes()`, `buildBundleAttributes()`, and `buildCall()` functions to simplify attribute construction.

#### Call-Level Attributes
These can be used in both `sendMessage` and individual call starters in `sendBundle`:

1. **interopCallValue**: Base token amount to send with the call on the destination chain
```solidity
attributes[0] = abi.encodeCall(IERC7786Attributes.interopCallValue, (1000));
```

2. **indirectCall**: Value for intermediate contract execution (used for token transfers and complex routing). We support custom base tokens across chains, so this attribute handles the bridging complexity.
```solidity
attributes[1] = abi.encodeCall(IERC7786Attributes.indirectCall, (0.1 ether));
```

#### Bundle-Level Attributes
These can only be used in `sendMessage` attributes or `sendBundle` bundle attributes (NOT in individual call starters):

3. **executionAddress**: Who can execute the bundle (permissioned execution)
```solidity
attributes[2] = abi.encodeCall(
    IERC7786Attributes.executionAddress,
    formatAddress(324, 0x1234...5678)
);
```

4. **unbundlerAddress**: Who can unbundle/cancel calls (permissioned unbundling)  
```solidity
attributes[3] = abi.encodeCall(
    IERC7786Attributes.unbundlerAddress,
    formatAddress(324, 0x8765...4321)
);
```

### Direct vs Indirect Calls

**Direct Calls**: Your call goes straight to the target contract on the destination chain.

```solidity
// Direct call - calling a DeFi function on another chain
bytes memory payload = abi.encodeCall(IDeFiProtocol.deposit, (amount));
InteropCenter.sendMessage(recipientAddress, payload, new bytes[](0));
```

**Indirect Calls**: Your call first goes through an intermediate contract which then generates the actual cross-chain call. This is essential for operations that need special handling, like token transfers.

Here's how the indirect call flow works:

```solidity
// Step 1: You call InteropCenter with the intermediate contract as recipient
bytes[] memory attributes = new bytes[](2);
attributes[0] = abi.encodeCall(IERC7786Attributes.indirectCall, (0.1 ether)); // msg.value for intermediate call
attributes[1] = abi.encodeCall(IERC7786Attributes.interopCallValue, (1000));   // value for final call

InteropCenter.sendMessage{value: 0.1 ether}(
    assetRouterAddress,      // This is the intermediate contract
    tokenTransferPayload,    // Instructions for the intermediate contract
    attributes
);
```

What happens under the hood (high level):
1. **On source chain**: InteropCenter calls `initiateIndirectCall` on the intermediate contract (e.g., AssetRouter)
2. **Intermediate contract**: Processes your request (locks tokens, prepares transfer data) and returns a new call starter
3. **InteropCenter**: Uses the returned call starter to create the actual cross-chain call
4. **On destination chain**: The final recipient gets called with the processed data

This pattern enables complex operations like:
- **Token transfers**: AssetRouter locks tokens on source, mints on destination
- **Cross-chain governance**: Intermediate contracts validate proposals and format calls
- **Bridged operations**: Intermediate contracts manage state synchronization

## FAQ

### Who pays for gas on the destination chain?

For cross-chain calls, the destination chain execution is paid by whoever calls `InteropHandler.executeBundle()`:
- **You** (if you set yourself as the execution address)
- **Anyone** (if no execution address specified - permissionless)
- **A relayer** (if you specified a relayer service)

For broadcast messages, verification is paid by whoever needs to prove the message.

### How fast is it?

- **Broadcast messages**: Available for verification after batch settlement.
- **Cross-chain calls**: Available for execution as soon as message containing information about that bundle is included in destination chain.
  1. Call is sent to L1 as a L2 -> L1 message
  2. Destination chain includes it
  3. Execution can happen now

### When to Use Single Calls vs Bundles

**Use `sendMessage` (single call) when:**
- You need to execute one function on another chain
- Simple token transfer or contract interaction
- You want the simplest API

**Use `sendBundle` (multiple calls) when:**
- You need atomic execution of multiple operations
- Complex workflows that must succeed or fail together
- You want to save on cross-chain overhead by batching

## Complex Scenario

Imagine building a cross-chain yield aggregator:
1. Broadcast current yields from multiple chains (using messages)
2. Transfer user funds to the best-yielding chain (indirect call via AssetRouter)
3. Deposit into the yield protocol (direct call)
4. Send receipt NFT back to original chain (another indirect call)

*Note: Complex cross-chain flows like this may require Account Abstraction (AA) for seamless user experience and proper transaction coordination.*

This combines both broadcast messages (for yield data) and cross-chain calls (for actual operations).

## Technical Details

### Architecture Overview

To summarize,

The interop system has multiple layers:

1. **Interop Messages**: Base broadcast layer (`sendToL1`)
2. **Interop Bundles**: Cross-chain call coordination (`InteropCenter`)
3. **Higher Layers**: Application-specific protocols (tokens, NFTs, etc.)

### Message Flow for Cross-Chain Calls

1. **Sending**: Calls wrapped in `InteropBundle` and sent via `InteropCenter`
2. **Propagation**: Bundle sent to L1, then to destination chain
3. **Verification**: Merkle proofs ensure authenticity
4. **Execution**: Bundles are processed via `InteropHandler`

### Bundle States

Cross-chain call bundles progress through:
- **Unreceived**: Not yet seen by destination
- **Verified**: Proven authentic, ready to execute
- **FullyExecuted**: All calls completed
- **Unbundled**: Partially executed or cancelled

### Security Features

- Cryptographic proofs for all cross-chain data
- Reentrancy protection
- Pausable for emergencies
- Permission system for execution control
- ERC-7786 and ERC-7930 standard compliances

The interop system provides flexible, secure cross-chain communication. Whether you're broadcasting data for verification, transferring assets, or orchestrating complex multi-chain operations, interop gives you the tools to build truly cross-chain applications.
