# ZKsync fee model

This document will assume that you already know how gas & fees work on Ethereum.

On Ethereum, all the computational, as well as storage costs, are represented via one unit: gas. Each operation costs a
certain amount of gas, which is generally constant (though it may change during
[upgrades](https://blog.ethereum.org/2021/03/08/ethereum-berlin-upgrade-announcement)).

ZKsync as well as other L2s have the issue which does not allow to adopt the same model as the one for Ethereum so
easily: the main reason is the requirement for publishing of the pubdata on Ethereum. This means that prices for L2
transactions will depend on the volatile L1 gas prices and can not be simply hardcoded.

## High-level description

ZKsync, being a zkRollup is required to prove every operation with zero knowledge proofs. That comes with a few nuances.

### `gas_per_pubdata_limit`

As already mentioned, the transactions on ZKsync depend on volatile L1 gas costs to publish the pubdata for batch,
verify proofs, etc. For this reason, ZKsync-specific EIP712 transactions contain the `gas_per_pubdata_limit` field in
them, denoting the maximum price in _gas_ that the operator \*\*can charge from users for a single byte of pubdata.

For Ethereum transactions (which do not contain this field), it is enforced that the operator will not use a value
larger value than a certain constant.

### Different opcode pricing

The operations tend to have different “complexity”/”pricing” in zero knowledge proof terms than in standard CPU terms.
For instance, `keccak256` which was optimized for CPU performance, will cost more to prove.

That’s why you will find the prices for operations on ZKsync a lot different from the ones on Ethereum.

### Different intrinsic costs

Unlike Ethereum, where the intrinsic cost of transactions (`21000` gas) is used to cover the price of updating the
balances of the users, the nonce and signature verification, on ZKsync these prices are _not_ included in the intrinsic
costs for transactions, due to the native support of account abstraction, meaning that each account type may have their
own transaction cost. In theory, some may even use more zk-friendly signature schemes or other kinds of optimizations to
allow cheaper transactions for their users.

That being said, ZKsync transactions do come with some small intrinsic costs, but they are mostly used to cover costs
related to the processing of the transaction by the bootloader which can not be easily measured in code in real-time.
These are measured via testing and are hard coded.

### Batch overhead & limited resources of the batch

In order to process the batch, the ZKsync team has to pay for proving of the batch, committing to it, etc. Processing a
batch involves some operational costs as well. All of these values we call “Batch overhead”. It consists of two parts:

- The L2 requirements for proving the circuits (denoted in L2 gas).
- The L1 requirements for the proof verification as well as general batch processing (denoted in L1 gas).

We generally try to aggregate as many transactions as possible and each transaction pays for the batch overhead
proportionally to how close did the transaction bring the batch to being _sealed,_ i.e. closed and prepared for proof
verification and submission on L1. A transaction gets closer to sealing a batch by using the batch’s _limited
resources_.

While on Ethereum, the main reason for the existence of batch gas limit is to keep the system decentralized & load low,
i.e. assuming the existence of the correct hardware, only time would be a requirement for a batch to adhere to. In the
case of ZKsync batches, there are some limited resources the batch should manage:

- **Time.** The same as on Ethereum, the batch should generally not take too much time to be closed in order to provide
  better UX. To represent the time needed we use a batch gas limit, note that it is higher than the gas limit for a
  single transaction.
- **Slots for transactions.** The bootloader has a limited number of slots for transactions, i.e. it can not take more
  than a certain transactions per batch.
- **The memory of the bootloader.** The bootloader needs to store the transaction’s ABI encoding in its memory & this
  fills it up. In practical terms, it serves as a penalty for having transactions with large calldata/signatures in case
  of custom accounts.
- **Pubdata bytes.** In order to fully appreciate the gains from the storage diffs, i.e. the fact that changes in a
  single slot happening in the same batch need to be published only once, we need to publish all the batch’s public data
  only after the transaction has been processed. Right now, we publish all the data with the storage diffs as well as
  L2→L1 messages, etc in a single transaction at the end of the batch. Most nodes have limit of 128kb per transaction
  and so this is the limit that each ZKsync batch should adhere to.

Each transaction spends the batch overhead proportionally to how close it consumes the resources above.

Note, that before the transaction is executed, the system can not know how many of the limited system resources the
transaction will actually take, so we need to charge for the worst case and provide the refund at the end of the
transaction.

### How `baseFee` works on ZKsync

In order to protect us from DDoS attacks we need to set a limited `MAX_TRANSACTION_GAS_LIMIT` per transaction. Since the
computation costs are relatively constant for us, we _could_ use a “fair” `baseFee` equal to the real costs for us to
compute the proof for the corresponding 1 erg. Note, that `gas_per_pubdata_limit` should be then set high enough to
cover the fees for the L1 gas needed to send a single pubdata byte on Ethereum. Under large L1 gas,
`gas_per_pubdata_limit` would also need be large. That means that `MAX_TRANSACTION_GAS_LIMIT/gas_per_pubdata_limit`
could become too low to allow for enough pubdata for lots of common use cases.

To make common transactions always executable, we must enforce that the users are always able to send at least
`GUARANTEED_PUBDATA_PER_TX` bytes of pubdata in their transaction. Because of that, the needed `gas_per_pubdata_limit`
for transactions should never grow beyond `MAX_TRANSACTION_GAS_LIMIT/GUARANTEED_PUBDATA_PER_TX`. Setting a hard bound on
`gas_per_pubdata_limit` also means that with the growth of L1 gas prices, the L2 `baseFee` will have to grow as well (to
ensure that `base_fee * gas_per_pubdata_limit = L1_gas_price * l1_gas_per_pubdata)`).

This does not actually matter a lot for normal transactions, since most of the costs will still go on pubdata for them.
However, it may matter for computationally intensive tasks, meaning that for them a big upfront payment will be
required, with the refund at the end of the transaction for all the overspent gas.

### Trusted gas limit

While it was mentioned above that the `MAX_TRANSACTION_GAS_LIMIT` is needed to protect the operator from users stalling
the state keeper by using too much computation, in case the users may need to use a lot of pubdata (for instance to
publish the bytecode of a new contract), the required gasLimit may go way beyond the `MAX_TRANSACTION_GAS_LIMIT` (since
the contracts can be 10s of kilobytes in size). All the new contracts to be published are included as part of the
factory dependencies field of the transaction and so the operator already knows how much pubdata will have to published
& how much gas will have to spent on it.

That’s why, to provide the better UX for users, the operator may provide the
[trusted gas limit](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L1137),
i.e. the limit which exceeds `MAX_TRANSACTION_GAS_LIMIT` assuming that the operator knows what he is doing (e.g. he is
sure that the excess gas will be spent on the pubdata).

### High-level: conclusion

The ZKsync fee model is meant to be the basis of the long-term fee model, which provides both robustness and security.
One of the most distinctive parts of it is the existing of the batch overhead, which is proportional for the resources
consumed by the transaction.

The other distinctive feature of the fee model used on ZKsync is the abundance of refunds, i.e.:

- For unused limited system resources.
- For overpaid computation.

This is needed because of the relatively big upfront payments required in ZKsync to provide DDoS security.

## Formalization

After determining price for each opcode in gas according to the model above, the following formulas are to be used for
calculating `baseFee` and `gasPerPubdata` for a batch.

#### System-wide constants

These constants are to be hardcoded and can only be changed via either system contracts/bootloader or VM upgrade.

`BATCH_OVERHEAD_L1_GAS` (*L*1*O*)— The L1 gas overhead for a batch (proof verification, etc).

`L1_GAS_PER_PUBDATA_BYTE` (*L*1*PUB*) — The number of L1 gas needed for a single pubdata byte. It is slightly higher
than 16 gas needed for publishing a non-zero byte of pubdata on-chain (currently the value of 17 is used).

`BATCH_OVERHEAD_L2_GAS` (_EO_)— The constant overhead denominated in gas. This overhead is created to cover the
amortized costs of proving.

`BLOCK_GAS_LIMIT` (_B_) — The maximum number of computation gas per batch. This is the maximal number of gas that can be
spent within a batch. This constant is rather arbitrary and is needed to prevent transactions from taking too much time
from the state keeper. It can not be larger than the hard limit of 2^32 of gas for VM.

`MAX_TRANSACTION_GAS_LIMIT` (_TM_) — The maximal transaction gas limit. For _i_-th single instance circuit, the price of
each of its units is $SC_i = \lceil \frac{T_M}{CC_i} \rceil$ to ensure that no transaction can run out of these single
instance circuits.

`MAX_TRANSACTIONS_IN_BATCH` (_TXM_) — The maximum number of transactions per batch. A constant in bootloader. Can
contain almost any arbitrary value depending on the capacity of batch that we want to have.

`BOOTLOADER_MEMORY_FOR_TXS` (_BM_) — The size of the bootloader memory that is used for transaction encoding
(i.e. excluding the constant space, preallocated for other purposes).

`GUARANTEED_PUBDATA_PER_TX` (_PG_) — The guaranteed number of pubdata that should be possible to pay for in one ZKsync
batch. This is a number that should be enough for most reasonable cases.

#### Derived constants

Some of the constants are derived from the system constants above:

`MAX_GAS_PER_PUBDATA` (_EPMax_) — the `gas_price_per_pubdata` that should always be enough to cover for publishing a
pubdata byte:

$$
EP_{Max} = \lfloor \frac{T_M}{P_G} \rfloor
$$

#### Externally-provided batch parameters

`L1_GAS_PRICE` (*L*1*P*) — The price for L1 gas in ETH.

`FAIR_GAS_PRICE` (_Ef_) — The “fair” gas price in ETH, that is, the price of proving one circuit (in Ether) divided by
the number we chose as one circuit price in gas.

$$
E_f = \frac{Price_C}{E_C}
$$

where _PriceC_ is the price for proving a circuit in ETH. Even though this price will generally be volatile (due to the
volatility of ETH price), the operator is discouraged to change it often, because it would volatile both volatile gas
price and (most importantly) the required `gas_price_per_pubdata` for transactions.

Both of the values above are currently provided by the operator. Later on, some decentralized/deterministic way to
provide these prices will be utilized.

#### Determining base_fee

When the batch opens, we can calculate the `FAIR_GAS_PER_PUBDATA_BYTE` (_EPf_) — “fair” gas per pubdata byte:

$$
EP_f = \lceil \frac{L1_p * L1_{PUB}}{E_f} \rceil
$$

There are now two situations that can be observed:

I.

$$
  EP_f > EP_{Max}
$$

This means that the L1 gas price is so high that if we treated all the prices fairly, then the number of gas required to
publish guaranteed pubdata is too high, i.e. allowing at least _PG_ pubdata bytes per transaction would mean that we
would to support _tx_._gasLimit_ greater that the maximum gas per transaction _TM_, allowing to run out of other finite
resources.

If $EP_f > EP_{Max}$, then the user needs to artificially increase the provided _Ef_ to bring the needed
_tx_._gasPerPubdataByte_ to _EPmax_

In this case we set the EIP1559 `baseFee` (_Base_):

$$
Base = max(E_f, \lceil \frac{L1_P * L1_{PUB}}{EP_{max}} \rceil)
$$

Only transactions that have at least this high gasPrice will be allowed into the batch.

II.

Otherwise, we keep $Base* = E_f$

Note, that both cases are covered with the formula in case (1), i.e.:

$$
Base = max(E_f, \lceil \frac{L1_P * L1_{PUB}}{EP_{max}} \rceil)
$$

This is the base fee that will be always returned from the API via `eth_gasGasPrice`.

#### Calculating overhead for a transaction

Let’s define by _tx_._actualGasLimit_ as the actual gasLimit that is to be used for processing of the transaction
(including the intrinsic costs). In this case, we will use the following formulas for calculating the upfront payment
for the overhead:

$$
S_O = 1/TX_M
$$

$$
M_O(tx) = encLen(tx) / B_M
$$

$$
E_{AO}(tx) = tx.actualGasLimit / T_M
$$

$$
O(tx) = max(S_O, M_O(tx), E_O(tx))
$$

where:

_SO_ — is the overhead for taking up 1 slot for a transaction

_MO_(_tx_) — is the overhead for taking up the memory of the bootloader

_encLen_(_tx_) — the length of the ABI encoding of the transaction’s struct.

_EAO_(_tx_) — is the overhead for potentially taking up the gas for single instance circuits.

_O_(_tx_) — is the total share of the overhead that the transaction should pay for.

Then we can calculate the overhead that the transaction should pay as the following one:

$$
L1_O(tx) = \lceil \frac{L1_O}{L1_{PUB}} \rceil * O(tx) \\
E_O(tx) = E_O * O(tx)
$$

Where

*L*1*O*(_tx_) — the number of L1 gas overhead (in pubdata equivalent) the transaction should compensate for gas.

_EO_(_tx_) — the number of L2 gas overhead the transaction should compensate for.

Then:

_overhead_\__gas_(_tx_) = *EO*(_tx_) + *tx*.*gasPerPubdata* ⋅ *L*1*O*(_tx_)

When a transaction is being estimated, the server returns the following gasLimit:

_tx_.*gasLimit* = *tx*.*actualGasLimit* + *overhead*\__gas_(_tx_)

Note, that when the operator receives the transaction, it knows only _tx_._gasLimit_. The operator could derive the
_overhead***gas*(*tx*) and provide the bootloader with it. The bootloader will then derive
*tx*.*actualGasLimit* = *tx*.*gasLimit* − *overhead***gas_(_tx_) and use the formulas above to derive the overhead that
the user should’ve paid under the derived _tx_._actualGasLimit_ to ensure that the operator does not overcharge the
user.

#### Note on formulas

For the ease of integer calculation, we will use the following formulas to derive the _overhead_(_tx_):

$B_O(tx) = E_O + tx.gasPerPubdataByte \cdot \lfloor \frac{L1_O}{L1_{PUB}} \rfloor$

$B_O$ denotes the overhead for batch in gas that the transaction would have to pay if it consumed the resources for
entire batch.

Then, _overhead_\__gas_(_tx_) is the maximum of the following expressions:

1. $S_O = \lceil \frac{B_O}{TX_M} \rceil$
2. $M_O(tx) = \lceil \frac{B_O \cdot encodingLen(tx)}{B_M} \rceil$
3. $E_O(tx) = \lceil \frac{B_O \cdot tx.gasBodyLimit}{T_M} \rceil$

#### Deriving `overhead_gas(tx)` from `tx.gasLimit`

The task that the operator needs to do is the following:

Given the tx.gasLimit, it should find the maximal `overhead_gas(tx)`, such that the bootloader will accept such
transaction, that is, if we denote by _Oop_ the overhead proposed by the operator, the following equation should hold:

$$
O_{op} ≤ overhead_gas(tx)
$$

for the $tx.bodyGasLimit$ we use the $tx.bodyGasLimit$ = $tx.gasLimit − O_{op}$.

There are a few approaches that could be taken here:

- Binary search. However, we need to be able to use this formula for the L1 transactions too, which would mean that
  binary search is too costly.
- The analytical way. This is the way that we will use and it will allow us to find such an overhead in O(1), which is
  acceptable for L1->L2 transactions.

Let’s rewrite the formula above the following way:

$$
O_{op} ≤ max(SO, MO(tx), EO(tx))
$$

So we need to find the maximal $O_{op}$, such that $O_{op} ≤ max(S_O, M_O(tx), E_O(tx))$. Note, that it means ensuring
that at least one of the following is true:

1. $O_{op} ≤ S_O$
2. $O_{op} ≤ M_O(tx)$
3. $O_{op} ≤ E_O(tx)$

So let’s find the largest _Oop_ for each of these and select the maximum one.

- Solving for (1)

$$
O_{op} = \lceil \frac{B_O}{TX_M} \rceil
$$

- Solving for (2)

$$
O_{op} = \lceil \frac{encLen(tx) \cdot B_O}{B_M} \rceil
$$

- Solving for (3)

This one is somewhat harder than the previous ones. We need to find the largest _O\_{op}_, such that:

$$
O_{op} \le \lceil \frac{tx.actualErgsLimit \cdot B_O}{T_M} \rceil   \\
$$

$$
O_{op} \le \lceil \frac{(tx.ergsLimit - O_{op}) \cdot B_O}{T_M} \rceil   \\
$$

$$
O_{op}  ≤ \lceil \frac{B_O \cdot (tx.ergsLimit - O_{op})}{T_M} \rceil
$$

Note, that all numbers here are integers, so we can use the following substitution:

$$
O_{op} -1 \lt \frac{(tx.ergsLimit - O_{op}) \cdot B_O}{T_M}    \\
$$

$$
(O_{op} -1)T_M \lt (tx.ergsLimit - O_{op}) \cdot B_O    \\
$$

$$
O_{op} T_M + O_{op} B_O \lt tx.ergsLimit \cdot B_O + T_M    \\
$$

$$
O_{op} \lt \frac{tx.ergsLimit \cdot B_O + T_M}{B_O + T_M}    \\
$$

Meaning, in other words:

$$
O_{op} = \lfloor \frac{tx.ergsLimit \cdot B_O + T_M - 1}{B_O + T_M} \rfloor
$$

Then, the operator can safely choose the largest one.

#### Discounts by the operator

It is important to note that the formulas provided above are to withstand the worst-case scenario and these are the
formulas used for L1->L2 transactions (since these are forced to be processed by the operator). However, in reality, the
operator typically would want to reduce the overhead for users whenever it is possible. For instance, in the server, we
underestimate the maximal potential `MAX_GAS_PER_TRANSACTION`, since usually the batches are closed because of either
the pubdata limit or the transactions’ slots limit. For this reason, the operator also provides the operator’s proposed
overhead. The only thing that the bootloader checks is that this overhead is _not larger_ than the maximal required one.
But the operator is allowed to provide a lower overhead.

#### Refunds

As you could see above, this fee model introduces quite some places where users may overpay for transactions:

- For the pubdata when L1 gas price is too low
- For the computation when L1 gas price is too high
- The overhead, since the transaction may not use the entire batch resources they could.

To compensate users for this, we will provide refunds for users. For all of the refunds to be provable, the counter
counts the number of gas that was spent on pubdata (or the number of pubdata bytes published). We will denote this
number by _pubdataused_. For now, this value can be provided by the operator.

The fair price for a transaction is

$$
FairFee = E_f \cdot tx.computationalGas + EP_f \cdot pubdataused
$$

We can derive $tx.computationalGas = gasspent − pubdataused \cdot tx.gasPricePerPubdata$, where _gasspent_ is the number
of gas spent for the transaction (can be trivially fetched in Solidity).

Also, the _FairFee_ will include the actual overhead for batch that the users should pay for.

The fee that the user has actually spent is:

$$
ActualFee = gasspent \cdot gasPrice
$$

So we can derive the overpay as

$$
ActualFee − FairFee
$$

In order to keep the invariant of $gasUsed \cdot gasPrice = fee$ , we will formally refund
$\frac{ActualFee - FairFee}{Base}$ gas.

At the moment, this counter is not accessible within the VM and so the operator is free to provide any refund it wishes
(as long as it is greater than or equal to the actual amount of gasLeft after the transaction execution).

#### Refunds for repeated writes

zkEVM is a statediff-based rollup, i.e. the pubdata is published not for transactions, but for storage changes. This
means that whenever a user writes into a storage slot, he incurs certain amount of pubdata. However, not all writes are
equal:

- If a slot has been already written to in one of the previous batches, the slot has received a short id, which allows
  it to require less pubdata in the state diff.
- Depending on the `value` written into a slot, various compression optimizations could be used and so we should reflect
  that too.
- Maybe the slot has been already written to in this batch and so we don’t to charge anything for it.

You can read more about how we treat the pubdata
[here](../settlement_contracts/data_availability/pubdata.md).

The important part here is that while such refunds are inlined (i.e. unlike the refunds for overhead they happen
in-place during execution and not after the whole transaction has been processed), they are enforced by the operator.
Right now, the operator is the one that decides what refund to provide.

## Improvements in the upcoming releases

The fee model explained above, while fully functional, has some known issues. These will be tackled with the following
upgrades.

### The quadratic overhead for pubdata

Note, that the computational overhead is proportional to the `tx.gasLimit` and the amount of funds the user will have to
pay is proportional to the L1 gas price (recall the formula of `B_O`). We can roughly express the transaction overhead
from computation as `tx.gasLimit * L1_GAS_PRICE * C` where `C` is just some constant. Note, that since a transaction
typically contains some storage writes, and its
`tx.gasLimit = gasSpentOnCompute + pubdataPublished * gasPricePerPubdata`, `tx.gasLimit` is roughly proportional to
`gasPricePerPubdata` and so it is also proportional to `L1_GAS_PRICE`.

This means that formula `tx.gasLimit * L1_GAS_PRICE * C` becomes _quadratic_ to the `L1_GAS_PRICE`.

### `gasUsed` depends to `gasLimit`

While in general it shouldn’t be the case assuming the correct implementation of [refunds](#refunds), in practice it
turned out that the formulas above, while robust, estimate for the worst case which can be very difference from the
average one. In order to improve the UX and reduce the overhead, the operator charges less for the execution overhead.
However, as a compensation for the risk, it does not fully refund for it.

### L1->L2 transactions do not pay for their execution on L1

The `executeBatches` operation on L1 is executed in `O(N)` where N is the number of priority ops that we have in the
batch. Each executed priority operation will be popped and so it incurs cost for storage modifications. As of now, we do
not charge for it.

## zkEVM Fee Components (Revenue & Costs)

- On-Chain L1 Costs
  - L1 Commit Batches
    - The commit batches transaction submits pubdata (which is the list of updated storage slots) to L1. The cost of a
      commit transaction is calculated as `constant overhead + price of pubdata`. The `constant overhead` cost is evenly
      distributed among L2 transactions in the L1 commit transaction, but only at higher transaction loads. As for the
      `price of pubdata`, it is known how much pubdata each L2 transaction consumed, therefore, they are charged
      directly for that. Multiple L1 batches can be included in a single commit transaction.
  - L1 Prove Batches
    - Once the off-chain proof is generated, it is submitted to L1 to make the rollup batch final. Currently, each proof
      contains only one L1 batch.
  - L1 Execute Batches
    - The execute batches transaction processes L2 -> L1 messages and marks executed priority operations as such.
      Multiple L1 batches can be included in a single execute transaction.
  - L1 Finalize Withdrawals
    - While not strictly part of the L1 fees, the cost to finalize L2 → L1 withdrawals are covered by Matter Labs. The
      finalize withdrawals transaction processes user token withdrawals from zkEVM to Ethereum. Multiple L2 withdrawal
      transactions are included in each finalize withdrawal transaction.
- On-Chain L2 Revenue
  - L2 Transaction Fee
    - This fee is what the user pays to complete a transaction on zkEVM. It is calculated as
      `gasLimit x baseFeePerGas - refundedGas x baseFeePerGas`, or more simply, `gasUsed x baseFeePerGas`.
- Profit = L2 Revenue - L1 Costs - Off Chain Infrastructure Costs