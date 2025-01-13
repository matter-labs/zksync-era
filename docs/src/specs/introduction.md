# Introduction

The goal of the ZK Stack is to power the internet of value. Value needs to be secured, and only blockchains are able
provide the level of security that the internet needs. The ZK Stack can be used to launch zero-knowledge rollups, which
are extra secure blockchains.

ZK Rollups use advanced mathematics called zero-knowledge proofs to show that the execution of the rollup was done
correctly. They also send ("roll up") their data to another chain, in our case this is Ethereum. The ZK Stack uses the
zkEVM to execute transactions, making it Ethereum compatible.

These two techniques allow the rollup to be verified externally. Unlike traditional blockchains, where you have to run a
node to verify all transactions, the state of the rollup can be easily checked by external participants by validating
the proof.

These external validators of a rollup can be other rollups. This means we can connect rollups trustlessly, and create a
network of rollups. This network is called the ZK Chain ecosystem.

These specs will provide a high level overview of the zkEVM and a full specification of its more technical components,
such as the prover, compiler, and the VM itself. We also specify the foundations of the ZK Chain ecosystem.
