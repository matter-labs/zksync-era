# Hyperchain upgrades

We encountered multiple issues while doing the upgrade. 
Initially when upgrading stage from v22->v23 ( upgrade stage folder) we noticed some issues in the server. We fixed those, and afterwards upgraded stage-proofs and testnet (stage-proofs and testnet folders) directly to v24. 

We noticed issues with the prover here. We upgraded testnet and stage-proofs directly without changing the protocol version (stage-proofs-fix, testnet-fix), just changing the Verification keys, and we also did a hot fix on the Executor facet for Validium (a further small issue was that due to the new contracts the upgrade scripts changed to handle upgrades happening through the STM).

We found a second similar issue in the prover, doing stage-proof-fix2 and testnet-fix2.

 We are upgrading mainnet directly from v22->v24 with the prover fixes (mainnet folder), and we will follow up with a stage upgrade v23->v24 (stage-24).   