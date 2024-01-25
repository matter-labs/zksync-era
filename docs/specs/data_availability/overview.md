# Overview

To support being a rollup, the ZK Stack needs to post the data of the chain on L1. Instead of submitting the data of
each transaction, we submit how the state of the blockchain changes, this change is called the state diff. This approach
allows the transactions that change the same storage slots to be very cheap, since these transactions don't incur
additional data costs.

Besides the state diff we also [post additional](./pubdata.md) data to L1, such as the L2->L1 messages, the L2->L1 logs,
the bytecodes of the deployed smart contracts.

We also [compress](./compression.md) all the data that we send to L1, to reduce the costs of posting it.

By posting all the data to L1, we can [reconstruct](./reconstruction.md) the state of the chain from the data on L1.
This is a key security property of the rollup.

The the chain chooses not to post this data, they become a validium. This makes transactions there much cheaper, but
less secure. Because we use state diffs to post data, we can combine the rollup and validium features, by separating
storage slots that need to post data from the ones that don't. This construction combines the benefits of rollups and
validiums, and it is called a [zkPorter](./validium_zk_porter.md).
