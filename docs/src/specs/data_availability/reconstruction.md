# L2 State Reconstruction Tool

Given that we post all data to L1, there is a tool, created by the [Equilibrium Team](https://equilibrium.co/) that
solely uses L1 pubdata for reconstructing the state and verifying that the state root on L1 can be created using
pubdata. A link to the repo can be found [here](https://github.com/eqlabs/zksync-state-reconstruct). The way the tool
works is by parsing out all the L1 pubdata for an executed batch, comparing the state roots after each batch is
processed.
