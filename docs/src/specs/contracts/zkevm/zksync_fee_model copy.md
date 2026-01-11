<!-- TODO review differences -->

# ZKsync fee model

This document will assume that you already know how gas & fees work on Ethereum.

On Ethereum, all the computational, as well as storage costs, are represented via one unit: gas. Each operation costs a certain amount of gas, which is generally constant (though it may change during [upgrades](https://blog.ethereum.org/2021/03/08/ethereum-berlin-upgrade-announcement)).

## Main differences from EVM

ZKsync as well as other L2s have the issue that does not allow the adoption of the same model as the one for Ethereum so easily: the main reason is the requirement for publishing the pubdata on Ethereum. This means that prices for L2 transactions will depend on the volatile L1 gas prices and can not be simply hard coded.

Also, ZKsync, being a zkRollup is required to prove every operation with zero-knowledge proofs. That comes with a few nuances.

### Different opcode pricing

The operations tend to have different “complexity”/”pricing” in zero-knowledge proof terms than in standard CPU terms. For instance, `keccak256` which was optimized for CPU performance, will cost more to prove.

That’s why you will find the prices for operations on ZKsync a lot different from the ones on Ethereum.

### I/O pricing

On Ethereum, whenever a storage slot is read/written to for the first time, a certain amount of gas is charged for the fact that the slot has been accessed for the first time. A similar mechanism is used for accounts: whenever an account is accessed for the first time, a certain amount of gas is charged for reading the account's data. On EVM, an account's data includes its nonce, balance, and code. We use a similar mechanism but with a few differences.

#### Storage costs

Just like EVM, we also support "warm" and "cold" storage slots. However, the flow is a bit different:

1. The user is firstly precharged with the maximum (cold) cost.
2. The operator is asked for a refund.
3. Then, the refund is given out to the user in place.

In other words, unlike EVM, the user should always have enough gas for the worst case (even if the storage slot is "warm"). Also, the control of the refunds is currently enforced by the operator only and not by the circuits.

#### Code decommitment and account access costs

Unlike EVM, our storage does not couple accounts' balances, nonces, and bytecodes. Balance, nonce, and code hash are three separate storage variables that use standard storage "warm" and "cold" mechanisms. A different approach is used for accessing bytecodes though.

We call the process of unpacking the bytecode as, _code decommitment_, since it is a process of transforming a commitment to code (i.e., the versioned code hash) into its preimage. Whenever a contract with a certain code hash is called, the following logic is executed:

1. The operator is asked whether this is the first time this bytecode has been decommitted.
2. If the operator returns "yes", then the user is charged the full cost. Otherwise, the user does not pay for decommit.
3. If needed, the code is decommitted to the code page.

Unlike storage interactions, the correctness of this process is _partially_ enforced by circuits, i.e., if step (3) is reached, i.e., the code is being decommitted, it will be proven that the operator responded correctly on step (1). However, if the program runs out of gas on step (2), the correctness of the first statement won't be proven. The reason for that is it is hard to prove in circuits at the time the decommitment is invoked whether it is indeed the first decommitment or not.

Note that in the case of an honest operator, this approach offers a better UX, since there is no need to be precharged with the full cost beforehand. However, no program should rely on this fact.

#### Conclusion

As a conclusion, ZKsync Era supports a similar "cold"/"warm" mechanism to EVM, but for now, these are only enforced by the operator, i.e., the users of the applications should not rely on these. The execution is guaranteed to be correct as long as the user has enough gas to pay for the worst, i.e. "cold" scenario.

### Memory pricing

ZKsync Era has different memory pricing rules:

- Whenever a user contract is called, `2^12` bytes of memory are given out for free, before starting to charge users linearly according to its length.
- Whenever a kernel space (i.e., a system) contract is called, `2^21` bytes of memory are given out for free, before starting to charge users linearly according to the length.
  Note that, unlike EVM, we never use a quadratic component of the price for memory expansion.

### Different intrinsic costs

Unlike Ethereum, where the intrinsic cost of transactions (`21000` gas) is used to cover the price of updating the balances of the users, the nonce and signature verification, on ZKsync these prices are _not_ included in the intrinsic costs for transactions, due to the native support of account abstraction, meaning that each account type may have their own transaction cost. In theory, some may even use more zk-friendly signature schemes or other kinds of optimizations to allow cheaper transactions for their users.

That being said, ZKsync transactions do come with some small intrinsic costs, but they are mostly used to cover costs related to the processing of the transaction by the bootloader which can not be easily measured in code in real-time. These are measured via testing and are hard coded.

### Charging for pubdata

An important cost factor for users is the pubdata. ZKsync Era is a state diff-based rollup, meaning that the pubdata is published not for the transaction data, but for the state changes: modified storage slots, deployed bytecodes, L2->L1 messages. This allows for applications that modify the same storage slot multiple times such as oracles, to update the storage slots multiple times while maintaining a constant footprint on L1 pubdata. Correctly a state diff rollups requires a special solution to charging for pubdata. It is explored in the next section.

## How L2 gas price works

### Batch overhead & limited resources of the batch

To process the batch, the ZKsync team has to pay for proving the batch, committing to it, etc. Processing a batch involves some operational costs as well. All of these values we call “Batch overhead”. It consists of two parts:

- The L2 requirements for proving the circuits (denoted in L2 gas).
- The L1 requirements for the proof verification as well as general batch processing (denoted in L1 gas).

We generally try to aggregate as many transactions as possible and each transaction pays for the batch overhead proportionally to how close the transaction brings the batch to being _sealed,_ i.e. closed and prepared for proof verification and submission on L1. A transaction gets closer to sealing a batch by using the batch’s _limited resources_.

While on Ethereum, the main reason for the existence of a batch gas limit is to keep the system decentralized & load low, i.e. assuming the existence of the correct hardware, only time would be a requirement for a batch to adhere to. In the case of ZKsync batches, there are some limited resources the batch should manage:

- **Time.** The same as on Ethereum, the batch should generally not take too much time to be closed to provide better UX. To represent the time needed we use a batch gas limit, note that it is higher than the gas limit for a single transaction.
- **Slots for transactions.** The bootloader has a limited number of slots for transactions, i.e. it can not take more than a certain transactions per batch.
- **The memory of the bootloader.** The bootloader needs to store the transaction’s ABI encoding in its memory & this fills it up. In practical terms, it serves as a penalty for having transactions with large calldata/signatures in the case of custom accounts.
- **Pubdata bytes.** To fully appreciate the gains from the storage diffs, i.e. the fact that changes in a single slot happening in the same batch need to be published only once, we need to publish all the batch’s public data only after the transaction has been processed. Right now, we publish all the data with the storage diffs as well as L2→L1 messages, etc in a single transaction at the end of the batch. Most nodes have a limit of 128kb per transaction, so this is the limit that each ZKsync batch should adhere to.

Each transaction spends the batch overhead proportionally to how closely it consumes the resources above.

Note, that before the transaction is executed, the system can not know how many of the limited system resources the transaction will take, so we need to charge for the worst case and provide the refund at the end of the transaction.

### `MAX_TRANSACTION_GAS_LIMIT`

A recommended maximal amount of gas that a transaction can spend on computation is `MAX_TRANSACTION_GAS_LIMIT`. But in case the operator trusts the user, the operator may provide the [trusted gas limit](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L1242), i.e. the limit which exceeds `MAX_TRANSACTION_GAS_LIMIT` assuming that the operator knows what he is doing. This can be helpful in the case of a hyperchain with different parameters.

### Derivation of `baseFee` and `gasPerPubdata`

At the start of each batch, the operator provides the following two parameters:

1. `FAIR_L2_GAS_PRICE`. This variable should denote what is the minimal L2 gas price that the operator is willing to accept. It is expected to cover the cost of proving/executing a single unit of zkEVM gas, the potential contribution of usage of a single gas towards sealing the batch, as well as congestion.
2. `FAIR_PUBDATA_PRICE`, which is the price of a single pubdata byte in Wei. Similar to the variable about, it is expected to cover the cost of publishing a single byte as well as the potential contribution of usage of a single pubdata byte towards sealing the batch.

In the descriptions above by "contribution towards sealing the batch" we referred to the fact that if a batch is most often closed by a certain resource (e.g. pubdata), then the pubdata price should include this cost.

The `baseFee` and `gasPerPubdata` are then calculated as:

```yul
baseFee := max(
    fairL2GasPrice,
    ceilDiv(fairPubdataPrice, MAX_L2_GAS_PER_PUBDATA())
)
gasPerPubdata := ceilDiv(pubdataPrice, baseFee)
```

While the way how we [charge for pubdata](#how-we-charge-for-pubdata) in theory allows for any `gasPerPubdata`, some SDKs expect the `gasLimit` by a transaction to be a uint64 number. We would prefer `gasLimit` for transactions to stay within JS's safe "number" range in case someone uses `number` type to denote gas there. For this reason, we will bind the `MAX_L2_GAS_PER_PUBDATA` to `2^20` gas per 1 pubdata byte. The number is chosen such that `MAX_L2_GAS_PER_PUBDATA * 2^32` is a safe JS integer. The `2^32` part is the maximal possible value for pubdata counter that could be in theory used. It is unrealistic that this value will ever appear under an honest operator, but it is needed just in case.

Note, however, that it means that the total under high L1 gas prices `gasLimit` may be larger than `u32::MAX` and it is recommended that no more than `2^20` bytes of pubdata can be published within a transaction.

#### Recommended calculation of `FAIR_L2_GAS_PRICE`/`FAIR_PUBDATA_PRICE`

Let's define the following constants:

- `BATCH_OVERHEAD_L1_GAS` - The L1 gas overhead for a batch (proof verification, etc).
- `COMPUTE_OVERHEAD_PART` - The constant that represents the possibility that a batch can be sealed because of overuse of computation resources. It has range from 0 to 1. If it is 0, the compute will not depend on the cost of closing the batch. If it is 1, the gas limit per batch will have to cover the entire cost of closing the batch.
- `MAX_GAS_PER_BATCH` - The maximum amount of gas that can be used by the batch. This value is derived from the circuits' limitation per batch.
- `PUBDATA_OVERHEAD_PART` - The constant that represents the possibility that a batch can be sealed because of overuse of pubdata. It has range from 0 to 1. If it is 0, the pubdata will not depend on the cost of closing the batch. If it is 1, the pubdata limit per batch will have to cover the entire cost of closing the batch.
- `MAX_PUBDATA_PER_BATCH` - The maximum amount of pubdata that can be used by the batch. Note that if the calldata is used as pubdata, this variable should not exceed 128kb.

And the following fluctuating variables:

- `MINIMAL_L2_GAS_PRICE` - The minimal acceptable L2 gas price, i.e. the price that should include the cost of computation/proving as well as potential premium for congestion.
- `PUBDATA_BYTE_ETH_PRICE` - The minimal acceptable price in ETH per each byte of pubdata. It should generally be equal to the expected price of a single blob byte or calldata byte (depending on the approach used).

Then:

1. `FAIR_L2_GAS_PRICE = MINIMAL_L2_GAS_PRICE + COMPUTE_OVERHEAD_PART * BATCH_OVERHEAD_L1_GAS / MAX_GAS_PER_BATCH`
2. `FAIR_PUBDATA_PRICE = PUBDATA_BYTE_ETH_PRICE + PUBDATA_OVERHEAD_PART * BATCH_OVERHEAD_L1_GAS / MAX_PUBDATA_PER_BATCH`

For L1→L2 transactions, the `MAX_GAS_PER_BATCH` variable is equal to `L2_TX_MAX_GAS_LIMIT` (since this amount of gas is enough to publish the maximal number of pubdata in the batch). Also, for additional security, for L1->L2 transactions the `COMPUTE_OVERHEAD_PART = PUBDATA_OVERHEAD_PART = 1`, i.e. since we are not sure what exactly will be the reason for us closing the batch. For L2 transactions, typically `COMPUTE_OVERHEAD_PART = 0`, since, unlike L1→L2 transactions, in case of an attack, the operator can simply censor bad transactions or increase the `FAIR_L2_GAS_PRICE` and so the operator can use average values for better UX.

#### Note on operator’s responsibility

To reiterate, the formulas above are used for L1→L2 transactions on L1 to protect the operator from malicious transactions. However, for L2 transactions, it is solely the responsibility of the operator to provide the correct values. It is designed this way for more fine-grained control over the system for the zkStack operators (including Validiums, maybe Era on top of another L1, etc).

This fee model also provides a very high degree of flexibility to the operator & so if we find out that we earn too much with a certain part, we could amend how the fair l2 gas price and fair pubdata price are generated and that’s it (there will be no further enforcements on the bootloader side).

In the long run, the consensus will ensure the correctness of these values on the main ZKsync Era (or maybe we’ll transition to a different system).

#### Overhead for transaction slot and memory

We also have a limit on the number of memory that can be consumed within a batch as well as the number of transactions that can be included there.

To simplify the codebase we've chosen the following constants:

- `TX_OVERHEAD_GAS = 10000` -- the overhead in gas for including a transaction into a batch.
- `TX_MEMORY_OVERHEAD_GAS = 10` -- the overhead for consuming a single byte of bootloader memory.

We've used roughly the following formulae to derive these values:

1. `TX_OVERHEAD_GAS = MAX_GAS_PER_BATCH / MAX_TXS_IN_BATCH`. For L1->L2 transactions we used the `MAX_GAS_PER_BATCH = 80kk` and `MAX_TXS_IN_BATCH = 10k`. `MAX_GAS_PER_BATCH / MAX_TXS_IN_BATCH = 8k`, while we decided to use the 10k value to better take into account the load on the operator from storing the information about the transaction.
2. `TX_MEMORY_OVERHEAD_GAS = MAX_GAS_PER_BATCH / MAX_MEMORY_FOR_BATCH`. For L1->L2 transactions we used the `MAX_GAS_PER_BATCH = 80kk` and `MAX_MEMORY_FOR_BATCH = 32 * 600_000`.

`MAX_GAS_PER_BATCH / MAX_MEMORY_FOR_BATCH = 4`, while we decided to use the `10` gas value to better take into account the load on the operator from storing the information about the transaction.

Future work will focus on removing the limit on the number of transactions’ slots completely as well as increasing the memory limit.

#### Note on L1→L2 transactions

The formulas above apply to L1→L2 transactions. However, note that the `gas_per_pubdata` is still kept as constant as `800`. This means that a higher `baseFee` could be used for L1->L2 transactions to ensure that `gas_per_pubdata` remains at that value regardless of the price of the pubdata.

#### Refunds

Note, that the used constants for the fee model are probabilistic, i.e. we never know in advance the exact reason why a batch is going to be sealed. These constants are meant to cover the expenses of the operator over a longer period so we do not refund the fact that the transaction might've been charged for overhead above the level at which the transaction has brought the batch to being closed, since these funds are used to cover transactions that did not pay in full for the limited batch's resources that they used.

#### Refunds for repeated writes

ZKsync Era is a state diff-based rollup, i.e. the pubdata is published not for transactions, but for storage changes. This means that whenever a user writes into a storage slot, it incurs a certain amount of pubdata. However, not all writes are equal:

- If a slot has been already written to in one of the previous batches, the slot has received a short ID, which allows it to require less pubdata in the state diff.
- Depending on the `value` written into a slot, various compression optimizations could be used and so we should reflect that too.
- Maybe the slot has been already written to in this batch so we don’t have to charge anything for it.

You can read more about how we treat the pubdata [here](../settlement_contracts/data_availability/standard_pubdata_format.md).

The important part here is that while such refunds are inlined (i.e. unlike the refunds for overhead they happen in place during execution and not after the whole transaction has been processed), they are enforced by the operator. Right now, the operator is the one who decides what refund to provide.

## How we charge for pubdata

ZKsync Era is a state diff-based rollup. It means that it is not possible to know how much pubdata a transaction will take before its execution. We _could_ charge for pubdata the following way: whenever a user does an operation that emits pubdata (writes to storage, publishes an L2->L1 message, etc.), we charge `pubdata_bytes_published * gas_per_pubdata` directly from the context of the execution.

However, such an approach has the following disadvantages:

- This would inherently make execution very divergent from EVM.
- It is prone to unneeded overhead. For instance, in the case of reentrancy locks, the user will still have to pay the initial price for marking the lock as used. The price will get refunded in the end, but it still worsens the UX.
- If we want to impose any sort of limit on how much computation a transaction could take (let's call this limit `MAX_TX_GAS_LIMIT`), it would mean that no more than `MAX_TX_GAS_LIMIT / gas_per_pubdata` could be published in a transaction, making this limit either too small or forcing us to increase `baseFee` to prevent the number from growing too much.

To avoid the issues above we need to somehow decouple the gas spent on pubdata from the gas spent on execution. While calldata-based rollups precharge for calldata, we cannot do it, since the exact state diffs are known only after the transaction is finished. We'll use the approach of _post-charging._ Basically, we'll keep a counter that tracks how much pubdata has been spent and charge the user for the calldata at the end of the transaction.

A problem with post-charging is that the user may spend all their gas within the transaction so we'll have no gas to charge for pubdata from. Note, however, that if the transaction is reverted, all the state changes that were related to it will be reverted too. That's why whenever we need to charge the user for pubdata, but it doesn't provide enough gas, the transaction will get reverted. The user will pay for the computation, but no state changes (and thus, pubdata) will be produced by the transaction.

So it will work the following way:

1. Firstly, we fix the amount of pubdata published so far. Let's denote it as `basePubdataSpent`.
2. We execute the validation of the transaction.
3. We check whether `(getPubdataSpent() - basePubdataSpent) * gasPerPubdata <= gasLeftAfterValidation`. If it is not, then the transaction does not cover enough funds for itself, so it should be _rejected_ (unlike revert, which means that the transaction is not even included in the block).
4. We execute the transaction itself.
5. We do the same check as in (3), but now if the transaction does not have enough gas for pubdata, it is reverted, i.e., the user still pays the fee to cover the computation for its transaction.
6. (optional, in case a paymaster is used). We repeat steps (4-5), but now for the `postTransaction` method of the paymaster.

On the internal level, the pubdata counter is modified in the following way:

- When there is a storage write, the operator is asked to provide by how much to increment the pubdata counter. Note that this value can be negative if, as in the example with a reentrancy guard, the storage diff is being reversed. There is currently no limit on how much the operator can charge for the pubdata.
- Whenever there is a need to publish a blob of bytes to L1 (for instance, when publishing a bytecode), the responsible system contract would increment the pubdata counter by `bytes_to_publish`.
- Whenever there is a revert in a frame, the pubdata counter gets reverted too, similar to storage & events.

The approach with post-charging removes the unneeded overhead and decouples the gas used for the execution from the gas used for data availability, which removes any caps on `gasPerPubdata`.

### Security considerations for protocol

Now it has become easier for a transaction to use up more pubdata than what can be published within a batch. In such a case, we'll revert the transaction as well.

### Security considerations for users

The approach with post-charging introduces one distinctive feature: it is not trivial to know the final price for a transaction at the time of its execution. When a user does `.call{gas: some_gas}` the final impact on the price of the transaction may be higher than `some_gas` since the pubdata counter will be incremented during the execution and charged only at the end of the transaction.

While for the average user, this limitation is not relevant, some specific applications may receive certain issues.

#### Example for a queue of withdrawals

Imagine that there is the following contract:

```solidity
struct Withdrawal {
   address token;
   address to;
   uint256 amount;
}

Withdrawals[] queue;
uint256 lastProcessed;

function processNWithdrawals(uint256 N) external nonReentrant {
  uint256 current = lastProcessed + 1;
  uint256 lastToProcess = current + N - 1;

  while(current <= lastToProcess) {
    // If the user provided some bad token that takes more than MAX_WITHDRAWAL_GAS
    // to transfer, it is the problem of the user and it will stall the queue, so
    // the `_success` value is ignored.
    Withdrawal storage currentQueue = queue[current];
    (bool _success, ) = currentQueue.token.call{gas: MAX_WITHDRAWAL_GAS}(abi.encodeWithSignature("transfer(to,amount)", currentQueue.to, currentQueue.amount));
    current += 1;
  }
  lastProcessed = lastToProcess;
}
```

The contract above supports a queue of withdrawals. This queue supports any type of token, including potentially malicious ones. However, the queue will never get stuck, since the `MAX_WITHDRAWAL_GAS` ensures that even if the malicious token does a lot of computation, it will be bound by this number and so the caller of the `processNWithdrawals` won't spend more than `MAX_WITHDRAWAL_GAS` per token.

The above assumptions work in the pre-charge model (calldata based rollups) or pay-as-you-go model (pre-1.5.0 Era). However, in the post-charge model, the `MAX_WITHDRAWAL_GAS`` limits the amount of computation that can be done within the transaction, but it does not limit the amount of pubdata that can be published. Thus, if such a function publishes a very large L1→L2 message, it might make the entire top transaction fail. This effectively means that such a queue would be stalled.

#### How to prevent this issue on the users' side

If a user really needs to limit the amount of gas that the subcall takes, all the subcalls should be routed through a special contract, that will guarantee that the total cost of the subcall wont be larger than the gas provided (by reverting if needed).

An implementation of this special contract can be seen [here](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/gas-bound-caller/contracts/GasBoundCaller.sol). Note, that this contract is _not_ a system one and it will be deployed on some fixed, but not kernel space address.

#### 1. Case of when a malicious contract consumes a large, but processable amount of pubdata\*\*

In this case, the topmost transaction will be able to sponsor such subcalls. When a transaction is processed, at most 80M gas is allowed to be passed to the execution. The rest can only be spent on pubdata during the post-charging.

#### 2. Case of when a malicious contract consumes an unprocessable amount of pubdata\*\*

In this case, the malicious callee published so much pubdata, that such a transaction can not be included into a batch. This effectively means that no matter how much money the topmost transaction willing to pay, the queue is stalled.

The only way how it is combated is by setting some minimal amount of ergs that still have to be consumed with each emission of pubdata (basically to make sure that it is not possible to publish large chunks of pubdata while using negligible computation). Unfortunately, setting this minimal amount to cover the worst possible case (i.e. 80M ergs spent with maximally 100k of pubdata available, leading to 800 L2 gas / pubdata byte) would likely be too harsh and will negatively impact average UX. Overall, this _is_ the way to go, however for now the only guarantee will be that a subcall of 1M gas is always processable, which will mean that at least 80 gas will have to be spent for each published pubdata byte. Even if higher than real L1 gas costs, it is reasonable even in the long run, since all the things that are published as pubdata are state-related and so they have to be well-priced for long-term storage.

In the future, we will guarantee the processability of subcalls of larger size by increasing the number of pubdata that can be published per batch.

### Limiting the `gas_per_pubdata`

As already mentioned, the transactions on ZKsync depend on volatile L1 gas costs to publish the pubdata for batch, verify proofs, etc. For this reason, ZKsync-specific EIP712 transactions contain the `gas_per_pubdata_limit` field, denoting the maximum `gas_per_pubdata` that the operator can charge the user for a single byte of pubdata.

For Ethereum transactions (which do not contain this field), the block's `gas_per_pubdata` is used.

## Improvements in the upcoming releases

The fee model explained above, while fully functional, has some known issues. These will be tackled with the following upgrades.

### L1->L2 transactions do not pay for their execution on L1

The `executeBatches` operation on L1 is executed in `O(N)` where N is the number of priority ops that we have in the batch. Each executed priority operation will be popped and so it incurs cost for storage modifications. As of now, we do not charge for it.

## ZKsync Era Fee Components (Revenue & Costs)

- On-Chain L1 Costs
  - L1 Commit Batches
    - The commit batch transaction submits pubdata (which is the list of updated storage slots) to L1. The cost of a commit transaction is calculated as `constant overhead + price of pubdata`. The `constant overhead` cost is evenly distributed among L2 transactions in the L1 commit transaction, but only at higher transaction loads. As for the `price of pubdata`, it is known how much pubdata each L2 transaction consumed, therefore, they are charged directly for that. Multiple L1 batches can be included in a single commit transaction.
  - L1 Prove Batches
    - Once the off-chain proof is generated, it is submitted to L1 to make the rollup batch final. Currently, each proof contains only one L1 batch.
  - L1 Execute Batches
    - The execute batches transaction processes L2 -> L1 messages and marks executed priority operations as such. Multiple L1 batches can be included in a single execute transaction.
  - L1 Finalize Withdrawals
    - While not strictly part of the L1 fees, the cost to finalize L2 → L1 withdrawals are covered by Matter Labs. The finalize withdrawals transaction processes user token withdrawals from ZKsync Era to Ethereum. Multiple L2 withdrawal transactions are included in each finalize withdrawal transaction.
- On-Chain L2 Revenue
  - L2 Transaction Fee
    - This fee is what the user pays to complete a transaction on ZKsync Era. It is calculated as `gasLimit x baseFeePerGas - refundedGas x baseFeePerGas`, or more simply, `gasUsed x baseFeePerGas`.
- Profit = L2 Revenue - L1 Costs - Off-Chain Infrastructure Costs
