# Netword design for zkSync-BFT

#  Introduction

A PoW based blockchain network is usually fully decentralized P2P network:

- Each block can be proposed by any node (full symmetry)
- Network is sparse (each node has a bounded number of connections)
- Network topology is unstructured (each node can decide independently to whom it wants to connect to), and quite randomized in practice
- Block production goes on even if the network is partitioned.

In contrast, zksync-BFT blockchain is supposed to be an L2 PoS blockchain, responsible for sequencing the transactions: availability of the blockchain state and censorship resistance are provided on a best effort basis (with a weight of stake on the L2 chain) as the strong guarantees on these properties are provided by the L1 Ethereum chain (thanks to the zkSync SNARK proofs and Ethereum calldata storage). In a sense, we could reduce our L2-blockchain to just leader election and it would be equally decentralized (and simpler in implementation). The problem with such a leader-election system would mean that the only notion of finality that we would get would be the one provided by L1 (which is O(hours) given the current zkSync implementation). zkSync L2 PoS chain is supposed to provide a fast “optimistic” finality (single party finality < **L2 finality** < L1 finality). That’s why it is important to reach the L2 consensus fast (like ~1s per block) and that requires a careful design of the network layer to get the messages between the consensus participants fast enough.

In contrast to PoW, a PoS network has a bounded (for example ~100) number of nodes with a special role: consensus participants (aka validators). The communication between them can be authenticated (their identity is encoded in the blockchain state itself) and it is critical for the new blocks to be produced (partitioning of the PoS validator network can stall the blockchain). Apart from validator <-> validator communication, a PoS network also requires “external world -> validator” communication (sending transactions to be included in the block), and “validator -> external world” communication (announcing the finalized blocks). This document outlines a design of the network stack for zkSync PoS L2 blockchain for these communication patterns (as well as some auxiliary communication patterns).

# Types of traffic

## Realtime BFT consensus

These are the HotStuff consensus messages. These are latency critical - the slower the consensus messages, the lower the block rate. For simplicity, we also consider these messages to be critical to deliver: although we only need ⅔ of the stake-weighted approvals, the analysis gets complicated if we do not consider each and every consensus message critical to deliver (for example, with 4 rounds of HotStuff per block, a MitM attack dropping less than ⅓ of stake-weighted messages per round can completely stall the blockchain anyway).

These consensus messages include leader -> replica messages:

- Prepare: block + block hash + quorum certificate x2 + signature
- Precommit, commit, decide: block hash + quorum certificate + signature

And replica -> leader messages:

- NewView: quorum certificate + signature
- Prepare, precommit, commit: block hash + signature

## Validator synchronization

If the validator is offline for some time, or has just joined the consensus (stakes can change per epoch), it has to collect enough information to participate in the consensus. In case of a stateless chain, it would be enough to just learn the last final block hash (+ view number). In case of a stateful chain (where block validity depends on the history), we need to fetch either the whole chain history, or chain history since the last available snapshot (which is either available locally or needs to be fetched as well). Snapshot fetching is not covered in this doc.

Validator can fetch the blocks that it is missing from other validators (the reliable way) or from an external source: like non-validator nodes, or some static storage (the best effort way). This document is only concerned with validator -> validator synchronization, as any best-effort alternative can be designed & developed independently. Depending on the broadcast strategy (whether we will introduce some redundancy, or just make the leader send a block directly to each replica) losing messages is more likely or less likely. No matter how many blocks are missing, the syncing validator should not overload other validators when asking for the missing blocks (i.e. a strict rate-limiting is required).

## Best effort realtime broadcast of blocks

Validators reach consensus on blocks, which then should be announced to the world. This part of communication is not critical (i.e. it doesn’t affect production of new blocks), but it is required to be realtime, so that the external systems can learn the new blocks ASAP (fast block finality is useful to external systems, only if they can observe it fast). Moreover (mostly for MEV) we will probably have to also broadcast the block proposals (i.e. non-final blocks) as well (if we don’t, people will start tweaking our code in unpredictable ways). However broadcasting the proposals (non-finalized blocks) is out of scope of this doc.

## Submitting transactions

This is the only kind of messages that can be sent from non-validator nodes to validator nodes.

Transactions are signed by the account owners, submitted to a non-validator node which then forwards them to the leader of the next BFT view. Depending on the network topology:

- Routing over non-validator nodes may be needed (in case the non-validator node is not directly connected to some validator)
- Routing/broadcast from a validator to other validators may be needed (in case the validator which receives the transaction is not currently the leader, so it cannot propose it in a block).

In case of PoS consensus, having a “mempool” of transactions isn’t strictly necessary, because the leader schedule is publicly known in advance, so the transaction signer can route the transaction directly to that leader (or a couple of soonest leaders for redundancy). The need for a mempool depends on the specific implementation though.

# Traffic load estimation

IMO it will be reasonable to design a solution with enough capacity to be sufficient for at least 2 years from now. To estimate the traffic patterns we need bounds on the following parameters:

- Number of validators
- Size of the block
- Desired block rate

This is obviously a simplification, which doesn’t take into account the local processing time, low-level network reliability, etc.

[Bruno França](mailto:bf@matterlabs.dev)

> Regarding the size estimates for transactions. It's actually hard to get that data for Arbitrum or Optimism, and getting that data for Ethereum is not going to be representative as we discussed. I decided to get a bit more creative. Most activity on rollups is going to be through smart contract wallets, and that's especially true for us since we support AA natively and are pushing for its adoption. So a better metric might be the size of an average smart contract transaction. I checked the Argent wallet relayer ([here](https://etherscan.io/address/0xf27696c8bca7d54d696189085ae1283f59342fa6)), saw a few transactions and calculated an average for different types of transactions:
> 
> 
> ETH transfer: 874 bytes ERC20 transfer: 954 bytes NFT transfer: 1114 bytes Uniswap swap: 2618 bytes
> 
> They are all in the same order of magnitude. Let's say that we expect that an average transaction will be 1 KB, given our estimate of 100 TPS, that's 100 KB/s.
> 

## System requirements

I suggest the following requirements:

- Number of validators: 100 - AFAIK large number of validators is usually good for marketing purposes (yay, so decentralized), but in practice the stake distribution isn’t uniform and ⅔ of stake is owned by a way lower number of validators. 100 seems to be a good number (AFAIK it matches the industry standards).
- Block size: estimate is ~100KB, but I’d suggest 1MB to give us some headroom.
- Block rate: ~2s to reach L2 finality of a new block. I guess that it won’t be a problem to have 2s/block at median, and sth like <10s/block at 99th percentile.

## Requirements for the validator operator

Reference hardware for other chains:

- [Polkadot](https://wiki.polkadot.network/docs/maintain-guides-how-to-validate-polkadot#reference-hardware)

The requirements that we put on a validator operator should be relatively low, so that it doesn’t turn out that it requires infrastructure provided by a handful of the largest cloud providers.

Note that these are just network requirements. Requirements for processing the blocks, etc., are not covered in this doc.

### Stable symmetric 1Gb/s connection

AFAIK that is already a consumer standard. BFT messages are not that large, but have non-negligible size. Note that this gives us 125MB/s, which is enough for a consensus leader to send the proposed block to every replica directly (<1MB block * 100 validators), if we ignore the fact that a BFT view requires multiple roundtrips. Since we estimate the load with a decent headroom, we will most likely be fine despite the simplifications in the analysis. Once the solution is implemented, we will perform a loadtest to verify our assumptions.

### <300ms round trip to other validators

This should cover most of the world, according to: https://wondernetwork.com/pings. This requirement is for the whole set of validators (not every single one individually). In practice, it means that for now we are OK excluding some parts of the world with very poor connectivity, and we are OK excluding some sophisticated setups (validators behind TOR network, multi-hop VPNs, etc.). We might revisit this assumption in the future if needed.

### A static public IP

To keep the latency low, we need the connections between the validators to be direct (unless we start aligning network topology geographically, but it will be hard). This requires each validator to have a public IP to which every other validator can connect to (I don’t think we want to support NAT hole punching via TURN protocol, because at the very least it would require trusting a third party).

In most setups having a transparent (cloud-like) firewall on the TCP/IP level should be enough to prevent DDoS attacks. If that’s not enough, we may facilitate a setup in which a validator also maintains a set of business-logic aware proxies (colocated with the validator), which act as a firewall/DDoS protection.

Requiring the public IP to be static will simplify the configuration (self-discovery usually requires a trusted third party) and reliability of the connections (dynamic IP change requires reestablishing a connection). In case the IP is ephemeral (static, but can change after restart) the reliability is not affected, but the configuration requires self-discovery.

# Network topology

## Validator network (TIER1)

Messages of the BFT consensus require low latency and reliable delivery (otherwise we won’t achieve our target block rate). For the number of validators between 100 to 1000 we can afford maintaining a separate direct TCP connection between every pair of validators (the system memory overhead is <10kB per connection, 1 TCP port, 1 file descriptor), we just need to make sure that TCP keepalive messages are sent rarely enough (that’s configurable). This will give us optimal latency (no proxy nodes in between) assuming the standard TCP/IP stack.

Validator connections can be authenticated, since the identity of every validator for the given epoch is publicly known (it is even available on the L1 chain) and the traffic pattern is predictable (although spiky, due to leader rotation). Therefore we can put a strict limit of 1 connection from each validator and set a strict quota on bandwidth of these connections.

With the assumptions that we made about the validator’s network bandwidth it is feasible for the leader to “broadcast” the messages to each of the replicas sequentially (no cross-replica broadcasting is needed). We can further optimize the latency of getting the approval responses, by sending the messages to replicas in decreasing order of stake. If the leader -> replica communication will turn out to be not efficient enough, we can redesign it later without affecting the larger design.

It is dangerous to assume that all N^2 = 100^2 = 10k connections will be available at all times. However, given our communication patterns we use just N out of these connections to be available at a given time (just connections [leader <-> all replicas] are relevant at any given time). Moreover, only at most ⅔ of these N connections are actually required to be available to reach the consensus (exact number depends on the stake distribution).

Although the consensus can be reached with very high probability, if some replica X is not connected to the leader L and doesn’t receive the proposal, it will fall behind. As soon as the view changes to the leader L’ that has a connection to X, replica X will learn that it is behind and will learn the new proposal. Given that the blockchain is stateful, X won’t be able to approve any proposal before it fetches all the ancestor blocks. But since it knows the HEAD of the chain it can fetch the ancestors by querying the other validators at random (fetching is sequential - grandparent will be known only once the parent is fetched).

### TIER1 bootstrapping

The identity of the validators will be known and stored on L1. Stake distribution can change only on the epoch boundary, so the same goes for the set of validators. The set of validators should be known an epoch in advance to avoid race conditions (if you are a new validator and you stake enough of your tokens at epoch X, you will become a consensus participant in epoch X+2). An epoch length would be hours or even days (TBD), while the IP of a validator may need to change way more often (dynamic/ephemeral IP, infrastructure migration, new node rollout, some emergency change). Moreover the IP address of a validator mustn’t be part of the chain state, otherwise we end up in a cyclic dependency where:

- Blockchain gets stalled => network configuration cannot change
- Network configuration change is required to reestablish the network to produce blockchain messages.

Hence the transportation layer (i.e. network) should be blockchain agnostic. This means that if a new validator node wants to join the TIER1 network, it has to first learn the IP endpoints of the current set of validators. We can implement it as follows:

- Each validator periodically (every few minutes) broadcasts the IP (signed with validator’s private key) at which it is currently available (periodically, so that it is easy to observe that the validator went entirely offline). Additionally, in case of a sudden IP change, validator may push information about the new IP immediately after the change (for example after restart).
- Every node of the network stores (in RAM) the mapping, validator -> IP, based on the received broadcasts. This mapping therefore becomes a piece of global state that is replicated across all the nodes in the network. It is OK, because the number of validators is strictly bounded, the state per validator is also strictly bounded ( <100kB per validator), so the whole global state is bounded as well. Note that this is a strictly stronger property than the one provided by [libp2p rendezvous spec](https://github.com/libp2p/specs/blob/master/rendezvous/README.md#spam-mitigation).
- Nodes exchange the full mapping “validator -> IP” when establishing new connections and periodically (every few minutes) to ensure eventual consistency despite any network problems.

This mechanism will work IF the network over which the IPs are broadcasted is APRIORI a connected graph. Because TIER1 network is dynamic (it changes every epoch) we cannot just use TIER1 itself for bootstrapping (yes, it would be a cyclical dependency) and say “every validator should first connect to one of the node X at IP Y”, because it would assume that X is always part of TIER1 network and moreover it would make X critical to be online when restarting a node. It would be slightly better in case instead we had a set of validators {X_1..X_k} or just to tell a new validator to figure out some of the current validators on their own. Also broadcasts become very inefficient when the network is dense (and in fact, the TIER1 network is supposed to be a full graph of connections), so it would be infeasible to do the “initial” and “periodic” full sync of the validator -> IP mapping in a naive way (N^2 communication cost). Note that we cannot utilize the concept of leader here, because the TIER1 bootstrapping should not depend on the blockchain state), yet it would be still possible to design some more fancy “broadcasting” strategy over TIER1 connections (it would introduce another layer of complexity though).

[recommended] However, it would be totally fine to utilize TIER2 for the validator discovery:

- TIER2 consists of both validator and non-validator nodes, which do not leave the network just because the epoch changed. In fact, non-validator nodes will usually be part of some high-reliability service, so they should have some quite decent uptime.
- TIER2 is expected to be significantly larger (I’d say 5-10 times larger) than the consensus itself and strongly connected. Just because of the size it should be harder to partition it due to random events.
- We may make TIER2 (as outlined above) contain a backbone based on local trust, which will give us quite a strong connectedness property, even under an attack.
- Main purpose of the TIER2 network is reliable broadcasts.

## Broadcast network (TIER2)

TIER1 nodes need to announce the blocks that the consensus has finalized to the external world. (validator -> external world communication). Non-validator nodes are exactly this “external world”. The number of non-validator is potentially unbounded and every service which wants to monitor the status of the blockchain real time is expected to have at least one. We can either:

- designate some kind of “hubs” that the non-validator nodes can connect to. For example, a validator may provide some “proxies” which would be able to handle a large number of inbound connections from the non-validator nodes. This puts an extra requirement on the validators and may affect the level of decentralization that our chain provides.
- [slightly recommended] Make the non-validator nodes connect into an unauthenticated P2P network: each non-validator node will connect to a bounded number of other non-validator nodes, roughly at random, creating a strongly-connected network with high probability. Validators will establish outbound connections to some of the non-validator nodes and broadcast the new blocks over these connections. Because the number of connections at each node is bounded, the broadcasts will be efficient in the number of messages and reliable due to strong-connectedness.

### TIER2 backbone [recommended]

Eclipse attack is a situation in which an adversary introduces a large number of nodes to the network, trying to partition it. For example, an adversary may try to convince a single honest node to connect to the adversary nodes and drop all other connections (effectively saturating the connection capacity of the honest node). Afterwards the adversary may perform MitM attack on the honest node, which in case of P2P broadcasts means censoring/dropping selectively the broadcasted messages.

Although the TIER2 network is unauthenticated by nature, we can utilize the concept of “local trust” to ensure connectedness (not necessarily strong) in case of an eclipse attack. “Local trust” means that a non-validator node operator knows (personally) the identity of a handful of other non-validator node operators, which they consider to be honest (we project the social trust between people onto trust between nodes). The identity of the “locally trusted” nodes (and their IPs) are then manually put into the configs (of nodes of both ends of the trust relation). The nodes will actively try to maintain connections to the trusted nodes regardless of the state of any other connections that they have (trusted connections are “fixed” and have dedicated capacity). The resulting network topology can be quite bad (it can have large diameter, the minimal cut may be low compared to a random network) but it constitutes a fixed backbone resistant to automated eclipse attacks (resistance to social engineering is out of scope of this doc).

The mechanism of local trust is also useful for reacting to emergencies. Every node (or group of nodes) may decide to reconfigure their fixed connections at will. In particular if we (matter labs) observe that something wrong is going on with our P2P network, we can ask the node operators to temporarily add a bunch of our nodes to their trusted nodes set, to improve the connectivity until the issue is resolved.

### Random TIER2 connections

As described in the previous section, the backbone based on local trust is not enough to establish a P2P network topology with good properties (low diameter, high connectedness).

The idea is roughly to connect to random nodes of the P2P network with uniform probability and then keep the connection stable for a long time (connection stability is important to attack resistance; TODO: elaborate on that). The set of all the nodes on the network is not known locally, so it is not that straightforward to draw a peer uniformly at random.

- Polkadot uses kademlia DHT: https://spec.polkadot.network/#sect-discovery-mechanism ([substrate docs](https://crates.parity.io/sc_network/index.html#discovery-mechanisms))
- Brahms algorithm: https://iditkeidar.com/wp-content/uploads/files/ftp/Brahms-PODC.pdf ([implementation analysis](https://www.net.in.tum.de/fileadmin/bibtex/publications/theses/totakura2015_brahms.pdf))
- A survey on crypto-currency networking: https://arxiv.org/pdf/2008.08412.pdf
- [related concept] [SCION](https://scion-architecture.net/) initiative

TODO: elaborate

### Routing transactions

“External world -> validator” communication will consist of new transactions to be submitted.

We have essentially 2 ways of implementing it:

- Assuming the TIER1 bootstrapping is implemented by utilizing TIER2, every node on the network knows the IPs of all the validators (and it even knows who is the current leader). Therefore a non-validator node may connect directly to the current/next leader and send the transaction. The connections would have to be short lived, so that a single validator can handle all of them, but assuming ~100 txns/block, implies 100 connections per second it should be manageable (if not, we can scale the validator to have some proxies to handle these).
    - Pros: high reliability of transaction delivery
    - Cons: validator is responsible to provide enough capacity to not get DDoS’ed. We allow non-validators (unauthenticated machines) to connect to validators (or their proxies).
- [slightly recommended] We may route the transaction over the TIER2 to the validator. To route the transaction, we need to implement/use some variant of [distance vector protocol](https://en.wikipedia.org/wiki/Distance-vector_routing_protocol). We can either monitor the distance to just 1 closest validator (non-validator -> … -> some validator -> leader) or to all the validators (non-validator -> … -> leader).
    - Pros: validators may keep just outbound connections to non-validator nodes this way. There are no ad hoc connections of unknown origin to the validators. We utilize the DDoS protection provided by the TIER2 network itself.
    - Cons: reliability of transaction delivery is limited by the properties of the distance vector protocol and the effective TIER2 topology (reliability can be increased by introducing some redundancy - sending multiple copies of messages over different routes). Increased latency.

## Synchronization network

A node (both validator and non-validator) may fall behind the chain head for various reasons:

1. It is a new node which has just been started and requires full sync. Since a blockchain is effectively a log of data which grows indefinitely, it is not feasible to fetch all the blocks since genesis for any long-lived chain, so the usual strategy is to checkpoint the chain state periodically, so that only the last checkpoint + recent blocks need to be fetched to sync a new node.
2. It was offline for some time (minutes,hours,days) and needs to fetch the blocks produced since the time it went offline. This will work when offline_time < checkpoint_period, otherwise the situation is equivalent to (1).
3. It is a validator node who just missed some of the recent blocks (network problems) and needs to fetch them ASAP to vote in the consensus. This is latency critical, so it should be handled by a separate mechanism (see “Validator network” section).

### Checkpoint/state synchronization

TODO

### Block synchronization

There is a fundamental trust issue in PoS blockchains: since the set of validators changes potentially even every epoch, and since the stake can be withdrawn, there is no economic incentive for past validators to NOT overwrite the history at will: their funds have already been withdrawn, so they can prepare a fork which is indistinguishable from the main fork of the chain.

- For L1 PoS chains the protection is the fact that the main fork is produced first and given reliable broadcasting, the whole network won’t ever accept the alternative chain which was produced hours/days after the original one.
- For L2 PoS chains the protection is the fact that the alternative fork won’t ever reach L1 finality, so at the point when the validators have withdrawn their funds, they cannot produce a valid (L1-finalizable) fork any more.

Hence, for the purpose of our L2 PoS we can assume that all blocks that we observe are either L1 finalized or protected by the L2 stake of the current validator set. From now on we assume that we need to synchronize blocks protected by L2 PoS stake (older blocks have been already aggregated into a checkpoint, so we can synchronize state at checkpoint instead).

Blockchain is a unidirectional linked list (with cryptohashes used instead of pointers) and there are essentially 2 sequential strategies for fetching them:

1. “The last finalized block that I know of is H, give me the finalized block whose parent is H”. It means that we fetch the blocks from oldest to newest, i.e. in the order that we need to apply them. It requires the node sending the response to maintain actually an index: block hash -> parent block, in addition to the standard: block hash -> block
2. “Give me the block at the current head of the chain” + “give me the parent of the given block”. It means that we fetch the blocks from newest to oldest. In this strategy applying the blocks can happen only after all blocks are fetched.

The main problem with these strategies is that the fetching is sequential (cannot be parallelized). This can be improved by turning our blockchain into a [skiplist](https://en.wikipedia.org/wiki/Skip_list). Alternatively we may utilize the fact that we can verify the finality of blocks out of order (any block with quorum on commit phase is guaranteed to be included in the chain) AND the fact our blocks include the ViewID: if the latest block that we know of has view V1, and the current head has view V2, we may simply query the peers to give us blocks with views in range (V1,V2). We can then partition this range into subranges to fetch the blocks in parallel. The exact strategy is outside of the scope of this doc.

Synchronization is a bandwidth heavy process, so we should use direct connections for synchronization (i.e. we shouldn’t route the payload, like in the “Routing transactions” section). Assuming that all the nodes on the network are trying to store all the recent blocks (blocks since the last checkpoint), it should be enough for our node to query just its direct TIER2 peers for the blocks (if they don’t have them, then also are in the sync mode)

- Fetching should be rate limited - requests from a single peer shouldn’t be able to overload the node. If the peer wants to sync faster it should just connect to more peers.
- Fetching shouldn’t affect the realtime broadcasts (i.e. broadcasting of the new blocks). We may want to use entirely separate TCP connections for syncing purposes to isolate the resources.

# Software stack

## Binary serialization format

Our system will consist of multiple independent nodes that we have little or no control over, so our network protocol needs to preserve backward compatibility across K release versions (where K has to be >=2). For our purposes protobuf ([rust implementation](https://docs.rs/protobuf/latest/protobuf/)) binary serialization format should be enough to achieve the desired level of backward compatibility (we can use [buf](https://buf.build/) as a presubmit check to ensure the low-level backward compatibility of the changes). Note that serialization format of network messages can be entirely different from the BFT consensus payload (aka “block”) serialization format. In particular the protobuf encoding is not unique, so hashing the protobuf messages doesn’t make sense, unless we apply some extra restrictions to it.

## RPC

I recommend using an RPC model for all the communication: in practice it means that each message sent (even if it is a broadcast message) should have a corresponding response message (sent within a predefined timeout) after the message is processed. It helps with monitoring responsiveness/efficiency of the peers and makes the control flow more synchronous (the more synchronous the code is, the easier it is to analyze).

Especially for TIER1, we can use gRPC [implementation](https://github.com/hyperium/tonic#features) which will give us rate-limiting, multiplexing, authentication, e2e encryption, health checking for free. It is built on top of hyper and utilizes HTTP2 (it will be even better once HTTP3 is supported, but that’s not critical). It is worth mentioning that it introduces a dependency on protoc though, which is hard/impossible to embed in the cargo build system (i.e. protoc is assumed to be available in the host system).

Alternatively we can use yamux from libp2p. TODO: elaborate.

## peer discovery

Tier2 will be an actual unauthenticated P2P network. As described in the previous sections, it will require peer discovery mechanism to provide a way to select peers (ideally, uniformly at random).

Options:

- kademlia from libp2p (todo: research more)
- implement variant of brahms (todo: research more)

## routing

TODO: describe options

- distance vector implementations
- kademlia?

# Related considerations (off-topic)

These are my thoughts more on the consensus protocol in general, rather than just for the transportation layer. Feel free to ignore it in favor of focusing on all the previous sections.

## Byte encoding for blocks

It would be nice to make it backward compatible just like the encoding of network messages (in a sense that new code can easily interpret old blocks). Here we have an additional requirement though, that the serialization should be unique (so that we can compute a hash out of it) OR ALTERNATIVELY that once a serialized block is signed, we pass it around without re-serializing it at any point. The important thing is to have a standard/widely used (preferably) language-agnostic serialization specification, which provides a tool for checking backward compatibility.

We definitely don’t want to provide infinite backward compatibility, because maintaining all historical behaviors is very expensive (for the developer) and error-prone. Since the state of our L2 chain will be checkpointed on the L1 chain, keeping an infinite history would be a waste of resources.

## Signing messages with validator keys

Cryptographic systems require the private key to be used to sign messages only of a single type: imagine a situation in which you use private key P to sign messages of type A and B, possibly in different contexts. If there is a payload which correctly deserializes to both value ‘a’ of type A and value ‘b’ of type B, then if you sign ‘a’ with key P, a malicious actor may present this signature to prove that you have signed ‘b’ instead.

For example, in this design we reuse the validator key to sign both BFT consensus messages AND to sign the validator discovery message (in section TIER1 bootstrapping). Therefore we should put either both message types into a single (protobuf?) enum, or use totally separate private keys for them.

## Node replication for reliability and rollouts

Validator operators will inevitably try to optimize their uptime.

Just spawning multiple instances of the node with the same validator key breaks the BFT consensus assumptions ([makes the validator malicious](https://wiki.polkadot.network/docs/maintain-guides-secure-validator#high-availability)), because it can accidentally sign conflicting messages. We should provide an out-of-the-box solution/deployment scheme which is BFT compatible and supports gradual rollouts.