# L2→L1 communication

The L2→L1 communication is more fundamental than the L1→L2 communication, as the second relies on the first. L2→L1
communication happens by the L1 smart contract verifying messages alongside the proofs. The only “provable” part of the
communication from L2 to L1 are native L2→L1 logs emitted by VM. These can be emitted by the `to_l1`
[opcode](../../../../../guides/advanced/12_alternative_vm_intro.md#only-for-kernel-space).
Each log consists of the following fields:

```solidity
struct L2Log {
  uint8 l2ShardId;
  bool isService;
  uint16 txNumberInBatch;
  address sender;
  bytes32 key;
  bytes32 value;
}

```

Where:

- `l2ShardId` is the id of the shard the opcode was called (it is currently always 0).
- `isService` a boolean flag that is not used right now
- `txNumberInBatch` the number of the transaction in the batch where the log has happened. This number is taken from the
  internal counter which is incremented each time the `increment_tx_counter` is
  [called](../../../../../guides/advanced/12_alternative_vm_intro.md#only-for-kernel-space).
- `sender` is the value of `this` in the frame where the L2→L1 log was emitted.
- `key` and `value` are just two 32-byte values that could be used to carry some data with the log.

The hashed array of these opcodes is then included into the
[batch commitment](https://github.com/matter-labs/era-contracts/blob/fdb60be1a49f5f0a371fc24b747cf5bdea1b1f74/l1-contracts/contracts/state-transition/chain-deps/facets/Committer.sol#L791-L797).
Because of that we know that if the proof verifies, then the L2→L1 logs provided by the operator were correct, so we can
use that fact to produce more complex structures. Before Boojum such logs were also Merklized within the circuits and so
the Merkle tree’s root hash was included into the batch commitment also.

## Important system values

Two `key` and `value` fields are enough for a lot of system-related use-cases, such as sending timestamp of the batch,
previous batch hash, etc. They were and are used
[used](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/SystemContext.sol#L438)
to verify the correctness of the batch's timestamps and hashes. You can read more about block processing
[here](../../../zkevm/batches_and_blocks_on_zksync.md).

## Long L2→L1 messages & bytecodes

However, sometimes users want to send long messages beyond 64 bytes which `key` and `value` allow us. But as already
said, these L2→L1 logs are the only ways that the L2 can communicate with the outside world. How do we provide long
messages?

Let’s add an `sendToL1` method in L1Messenger, where the main idea is the following:

- Let’s submit an L2→L1 log with `key = msg.sender` (the actual sender of the long message) and
  `value = keccak256(message)`.
- Now, during batch commitment the operator will have to provide an array of such long L2→L1 messages and it will be
  checked on L1 that indeed for each such log the correct preimage was provided.

A very similar idea is used to publish uncompressed bytecodes on L1 (the compressed bytecodes were sent via the long
L1→L2 messages mechanism as explained above).

Note, however, that whenever someone wants to prove that a certain message was present, they need to compose the L2→L1
log and prove its presence.

## Priority operations

Also, for each priority operation, we would send its hash and it status via an L2→L1 log. On L1 we would then
reconstruct the rolling hash of the processed priority transactions, allowing to correctly verify during the
`executeBatches` method that indeed the batch contained the correct priority operations.

Importantly, the fact that both hash and status were sent, it made it possible to
[prove](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/contracts/ethereum/contracts/bridge/L1ERC20Bridge.sol#L255)
that the L2 part of a deposit has failed and ask the bridge to release funds.
