# Fees (a.k.a gas)

What is the L2 gas price? It's **0.1 Gwei** (and as we improve our provers/VM we hope it will go down). However, it can
vary at times. Please see further information below.

## What do you pay for

The gas fee covers the following expenses:

- Calculation and storage (related to most operations)
- Publishing data to L1 (a significant cost for many transactions, with the exact amount depending on L1)
- Sending 'bytecode' to L1 (if not already there) - typically a one-time cost when deploying a new contract
- Closing the batch and handling proofs - This aspect also relies on L1 costs (since proof publication must be covered).

## Price configuration

We have two pricing models (old and new):

- the `L1Pegged` - until protocol version 19
- the `PubdataIndependent` - from protocol version 20 (release 1.4.1)

### L1 pegged ('old' fee model)

Under this fee model, operator was providing `FeeParamsV1`, which contained:

- l1_gas_price
- minimal_l2_gas_price

then, the system was computing `L1PeggedBatchFeeModelInput`, that contained

- l1_gas_price
- 'fair' l2 gas price - which in 99% of cases was equal to minimal_gas_price, and was greater from it only if l1 gas
  price was huge, to guarantee that we can publish enough data in each transaction (see table below for details).

Many other values, were 'hardcoded' within the system (for example how to compute the pubdata price based on l1 gas
price, how much committing the proof to L1 costs etc).

### PubdataIndependent ('new' fee model)

This method is called `PubdataIndependent` and the change was done to allow more flexibility in pubdata costs (for
example if pubdata is published to another Data Availability layer, or if not published at all - in case of validium).

In this model, there are 8 config options, let's walk through them:

`FeeParamsV2` contains 2 dynamic prices:

- `l1_gas_price` - which is used to compute the cost of submitting the proofs on L1
- `l1_pubdata_price` - which is the cost of submitting a single byte of pubdata

And config options (`FeeModelConfigV2`) contain:

- `minimal_l2_gas_price` - similar meaning to the one in the previous model - this should cover the $ costs of running
  the machines (node operator costs).

2 fields around the maximum capacity of the batch (note - that these are used only for the fee calculation, the actual
sealing criteria are specified in different configuration):

- `max_gas_per_batch` - expected maximum amount of gas in a batch
- `max_pubdata_per_batch` - expected maximum amount of pubdata that we can put in a single batch - due to Ethereum
  limitations (usually it is around 128kb when using calldata, and 250 when using 2 blobs)

the actual cost of the batch:

- `batch_overhead_l1_gas` - how much gas operator will have to pay to handle the proof on L1 (this should include
  commitBatch, proveBatch and executeBatch costs). This should NOT include any cost that is related to pubdata.

And 2 fields about who contributed to closing the batch:

- `pubdata_overhead_part` - from 0 - 1
- `compute_overhead_part` - from 0 - 1

#### Cost distribution between transactions

Batches are closed, when we either run out of circuits (a.k.a gas / computation), or run out of pubdata capacity (too
much data to publish to L1 in one transaction), or run out of transactions slots (which should be a rare event with
recent improvements)

Closing each batch, has some cost for the operator - especially the one related to L1 costs - where operator has to
submit a bunch of transactions, including the one to verify the proof (these costs are counted in
`batch_overhead_l1_gas`).

Now the problem that operator is facing is, who should pay for closing of the batch. In a perfect world, we'd look at
the reason for batch closure (for example pubdata) - and then charge the transactions proportionally to the amount of
pubdata that they used. Unfortunately this is not feasible, as we have to charge the transactions as we go, rather than
at the end of the batch (that can have 1000s of transactions).

That's why we have the logic of `pubdata_overhead_part` and `compute_overhead_part`. These represent the 'odds' whether
pubdata or compute were the reason for the batch closure - and based on this information, we distribute the costs to
transactions:

```
cost_of_closing_the_batch = (compute_overhead_part * TX_GAS_USED / MAX_GAS_IN_BLOCK + pubdata_overhead_part * PUBDATA_USED / MAX_PUBDATA_IN_BLOCK)
```

#### Custom base token configurations

When running a system based on a custom token, all the gas values above, should refer to YOUR custom token.

Example, if you're running USDC as base token, and ETH currently costs 2000$, and current L1 gas price is 30 Gwei.

- `l1_gas_price` param should be set to 30 \* 2000 == 60'000 Gwei
- `l1_pubdata_price` should be also updated accordingly (also multiplied by 2'000). (note: currently bootloader has a
  cap of 1M gwei per pubdata price - and we're working on removing this limitation)
- `minimal_l2_gas_price` should be set in such way, that `minimal_l2_gas_price * max_gas_per_batch / 10**18 $` is enough
  to pay for your CPUs and GPUs

#### Validium / Data-availability configurations

If you're running a system with validium without any DA, you can just set the `l1_pubdata_price` to 0,
`max_pubdata_per_batch` to some large value, and set `pubdata_overhead_part` to 0, and `compute_overhead_part` to 1.

If you're running alternative DA, you should adjust the `l1_pubdata_price` to roughly cover the cost of writing one byte
to the DA, and set `max_pubdata_per_batch` to the DA limits.

Note: currently system still requires operator to keep the data in memory and compress it, which means that setting huge
values of `max_pubdata_per_batch` might not work. This will be fixed in the future.

Assumption: ETH costs 2'000$, L1 gas cost is 30 Gwei, blob costs 2Gwei (per byte), and DA allows 1MB payload that cost 1
cent.

| flag                    | rollup with calldata | rollup with 4844 (blobs) | value for validium | value for DA |
| ----------------------- | -------------------- | ------------------------ | ------------------ | ------------ |
| `l1_pubdata_price`      | 510'000'000'000      | 2'000'000'000            | 0                  | 5'000        |
| `max_pubdata_per_batch` | 120'000              | 250'000                  | 1'000'000'000'000  | 1'000'000    |
| `pubdata_overhead_part` | 0.7                  | 0.4                      | 0                  | 0.1          |
| `compute_overhead_part` | 0.5                  | 0.7                      | 1                  | 1            |
| `batch_overhead_l1_gas` | 1'000'000            | 1'000'000                | 1'000'000          | 1'400'000    |

The cost of l1 batch overhead is higher for DA, as it has to cover the additional costs of checking on L1 if DA actually
got the data.

## L1 vs L2 pricing

Here is a simplified table displaying various scenarios that illustrate the relationship between L1 and L2 fees:

| L1 gas price | L2 'minimal price' | L2 'gas price' | L2 gas per pubdata | Note                                                                                                                                                  |
| ------------ | ------------------ | -------------- | ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.25 Gwei    | 0.25 Gwei          | 0.25 Gwei      | 17                 | Gas prices are equal, so the charge is 17 gas, just like on L1.                                                                                       |
| 10 Gwei      | 0.25 Gwei          | 0.25 Gwei      | 680                | L1 is 40 times more expensive, so we need to charge more L2 gas per pubdata byte to cover L1 publishing costs.                                        |
| 250 Gwei     | 0.25 Gwei          | 0.25 Gwei      | 17000              | L1 is now very expensive (1000 times more than L2), so each pubdata costs a lot of gas.                                                               |
| 10000 Gwei   | 0.25 Gwei          | 8.5 Gwei       | 20000              | L1 is so expensive that we have to raise the L2 gas price, so the gas needed for publishing doesn't exceed the 20k limit, ensuring L2 remains usable. |

**Why is there a 20k gas per pubdata limit?** - We want to make sure every transaction can publish at least 4kb of data
to L1. The maximum gas for a transaction is 80 million (80M/4k = 20k).

### L2 Fair price

The L2 fair gas price is currently determined by the StateKeeper/Sequencer configuration and is set at 0.10 Gwei (see
`fair_l2_gas_price` in the config). This price is meant to cover the compute costs (CPU + GPU) for the sequencer and
prover. It can be changed as needed, with a safety limit of 10k Gwei in the bootloader. Once the system is
decentralized, more deterministic rules will be established for this price.

### L1 Gas price

The L1 gas price is fetched by querying L1 every 20 seconds. This is managed by the [`GasAdjuster`][gas_adjuster], which
calculates the median price from recent blocks and enables more precise price control via the config (for example,
adjusting the price with `internal_l1_pricing_multiplier` or setting a specific value using
`internal_enforced_l1_gas_price`).

### Overhead gas

As mentioned earlier, fees must also cover the overhead of generating proofs and submitting them to L1. While the
detailed calculation is complex, the short version is that a full proof of an L1 batch costs around **1 million L2 gas,
plus 1M L1 gas (roughly equivalent of 60k published bytes)**. In every transaction, you pay a portion of this fee
proportional to the part of the batch you are using.

## Transactions

| Transaction Field | Conditions                             | Note                                                                                                                 |
| ----------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| gas_limit         | `<= max_allowed_l2_tx_gas_limit`       | The limit (4G gas) is set in the `StateKeeper` config; it's the limit for the entire L1 batch.                       |
| gas_limit         | `<= MAX_GAS_PER_TRANSACTION`           | This limit (80M) is set in bootloader.                                                                               |
| gas_limit         | `> l2_tx_intrinsic_gas`                | This limit (around 14k gas) is hardcoded to ensure that the transaction has enough gas to start.                     |
| max_fee_per_gas   | `>= fair_l2_gas_price`                 | Fair L2 gas price (0.1 Gwei on Era) is set in the `StateKeeper` config                                               |
|                   | `<=validation_computational_gas_limit` | There is an additional, stricter limit (300k gas) on the amount of gas that a transaction can use during validation. |

### Why do we have two limits: 80M and 4G

The operator can set a custom transaction limit in the bootloader. However, this limit must be within a specific range,
meaning it cannot be less than 80M or more than 4G.

### Why validation is special

In Ethereum, there is a fixed cost for verifying a transaction's correctness by checking its signature. However, in
ZKsync, due to Account Abstraction, we may need to execute some contract code to determine whether it's ready to accept
the transaction. If the contract rejects the transaction, it must be dropped, and there's no one to charge for that
process.

Therefore, a stricter limit on validation is necessary. This prevents potential DDoS attacks on the servers, where
people could send invalid transactions to contracts that require expensive and time-consuming verifications. By imposing
a stricter limit, the system maintains stability and security.

## Actual gas calculation

From the Virtual Machine (VM) point of view, there is only a bootloader. When executing transactions, we insert the
transaction into the bootloader memory and let it run until it reaches the end of the instructions related to that
transaction (for more details, refer to the 'Life of a Call' article).

To calculate the gas used by a transaction, we record the amount of gas used by the VM before the transaction execution
and subtract it from the remaining gas after the execution. This difference gives us the actual gas used by the
transaction.

```rust
let gas_remaining_before = vm.gas_remaining();
execute_tx();
let gas_used = gas_remaining_before - vm.gas_remaining();
```

## Gas estimation

Before sending a transaction to the system, most users will attempt to estimate the cost of the request using the
`eth_estimateGas` call.

To estimate the gas limit for a transaction, we perform a binary search (between 0 and the `MAX_L2_TX_GAS_LIMIT` of 80M)
to find the smallest amount of gas under which the transaction still succeeds.

For added safety, we include some 'padding' by using two additional config options: `gas_price_scale_factor` (currently
1.5) and `estimate_gas_scale_factor` (currently 1.3). These options are used to increase the final estimation.

The first option simulates the volatility of L1 gas (as mentioned earlier, high L1 gas can affect the actual gas cost of
data publishing), and the second one serves as a 'safety margin'.

You can find this code in [get_txs_fee_in_wei][get_txs_fee_in_wei] function.

## Q&A

### Is ZKsync really cheaper

In short, yes. As seen in the table at the beginning, the regular L2 gas price is set to 0.25 Gwei, while the standard
Ethereum price is around 60-100 Gwei. However, the cost of publishing to L1 depends on L1 prices, meaning that the
actual transaction costs will increase if the L1 gas price rises.

### Why do I hear about large refunds

There are a few reasons why refunds might be 'larger' on ZKsync (i.e., why we might be overestimating the fees):

- We must assume (pessimistically) that you'll have to pay for all the slot/storage writes. In practice, if multiple
  transactions touch the same slot, we only charge one of them.
- We have to account for larger fluctuations in the L1 gas price (using gas_price_scale_factor mentioned earlier) - this
  might cause the estimation to be significantly higher, especially when the L1 gas price is already high, as it then
  impacts the amount of gas used by pubdata.
  
[gas_adjuster]:
  https://github.com/matter-labs/zksync-era/blob/main/core/node/fee_model/src/l1_gas_price/gas_adjuster/mod.rs#L50
  'gas_adjuster'
[get_txs_fee_in_wei]:
  https://github.com/matter-labs/zksync-era/blob/714a8905d407de36a906a4b6d464ec2cab6eb3e8/core/lib/zksync_core/src/api_server/tx_sender/mod.rs#L656
  'get_txs_fee_in_wei'
