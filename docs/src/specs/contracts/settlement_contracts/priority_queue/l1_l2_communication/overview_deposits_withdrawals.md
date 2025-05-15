# Overview - Deposits and Withdrawals

The zkEVM supports general message passing for L1<->L2 communication. Proofs are settled on L1, so core of this process
is the L2->L1 message passing process. L1->L2 messages are recorded on L1 inside a priority queue, the sequencer picks
it up from here and executes it in the zkEVM. The zkEVM sends an L2->L1 message of the L1 transactions that it
processed, and the rollup's proof is only valid if the processed transactions were exactly right.

There is an asymmetry in the two directions however, in the L1->L2 direction we support starting message calls by having
a special transaction type called L1 transactions. In the L2->L1 direction we only support message passing.

In particular, deposits and withdrawals of ether also use the above methods. For deposits the L1->L2 transaction is sent
with empty calldata, the recipients address and the deposited value. When withdrawing, an L2->L1 message is sent. This
is then processed by the smart contract holding the ether on L1, which releases the funds.

The details are covered in the [interop center](../../../interop/interop_center/overview.md) section.