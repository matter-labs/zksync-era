# Validium and zkPorter

The may choose not to post their data to L1, in which case they become a validium. This makes transactions there much
cheaper, but less secure. Because the ZK Stack uses state diffs to post data, it can combine the rollup and validium
features, by separating storage slots that need to post data from the ones that don't. This construction combines the
benefits of rollups and validiums, and it is called a
[zkPorter](https://blog.matter-labs.io/zkporter-composable-scalability-in-l2-beyond-zkrollup-2a30c4d69a75).
