# Decentralization

In the default setup, the ZKsync node will fetch data from the ZKsync API endpoint maintained by Matter Labs. To reduce
the reliance on this centralized endpoint we have developed a decentralized p2p networking stack (aka gossipnet) which
will eventually be used instead of ZKsync API for synchronizing data.

On the gossipnet, the data integrity will be protected by the BFT (byzantine fault-tolerant) consensus algorithm
(currently data is signed just by the main node though).

### Add `--enable-consensus` flag to your entry point command

For the consensus configuration to take effect you have to add `--enable-consensus` flag when running the node. You can
do that by editing the docker compose files (mainnet-external-node-docker-compose.yml or
testnet-external-node-docker-compose.yml) and uncommenting the line with `--enable-consensus`.
