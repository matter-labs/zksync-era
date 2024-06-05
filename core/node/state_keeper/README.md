# `zksync_state_keeper`

State keeper is the main component of the sequencer implementation. Its main responsibility is to extract transactions
from a certain source (like mempool), form them into a set of L2 blocks and L1 batches, and pass for persisting and
further processing.
