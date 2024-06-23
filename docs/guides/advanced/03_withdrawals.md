# ZKsync deeper dive bridging stuff back (a.k.a withdrawals)

Assuming that you have completed [part 1](01_initialization.md) and [part 2](02_deposits.md) already, we can bridge the
tokens back by simply calling the zksync-cli:

```bash
npx zksync-cli bridge withdraw --chain=dockerized-node
```

And providing the account name (public address) and private key.

Afterward, by using `web3` tools, we can quickly check that funds were transferred back to L1. **And you discover that
they didn't** - what happened?

Actually we'll have to run one additional step:

```bash
npx zksync-cli bridge withdraw-finalize --chain=dockerized-node
```

and pass the transaction that we received from the first call, into the `withdraw-finalize` call.

**Note:** This is not needed on testnet - as we (MatterLabs) - are running an automatic tool that confirms withdrawals.

### Looking deeper

But let's take a look what happened under the hood.

Let's start by looking at the output of our `zksync-cli`:

```
Withdrawing 7ETH to 0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd on localnet
Transaction submitted 💸💸💸
L2: tx/0xe2c8a7beaf8879cb197555592c6eb4b6e4c39a772c3b54d1b93da14e419f4683
Your funds will be available in L1 in a couple of minutes.
```

**important** - your transaction id will be different - make sure that you use it in the methods below.

The tool created the withdraw transaction and it sent it directly to our server (so this is a L2 transaction). The zk
server has received it, and added it into its database. You can check it by querying the `transactions` table:

```shell
# select * from transactions where hash = '\x<YOUR_L2_TRANSACTION_ID_FROM_ABOVE>`
select * from transactions where hash = '\xe2c8a7beaf8879cb197555592c6eb4b6e4c39a772c3b54d1b93da14e419f4683';
```

This will print a lot of columns, but let's start by looking at the `data` column:

```json
{
  "value": "0x6124fee993bc0000",
  "calldata": "0x51cff8d9000000000000000000000000618263ce921f7dd5f4f40c29f6c524aaf97b9bbd",
  "factoryDeps": null,
  "contractAddress": "0x000000000000000000000000000000000000800a"
}
```

We can use the ABI decoder tool <https://calldata-decoder.apoorv.xyz/> to see what this call data means:

```json
{
  "function": "withdraw(address)",
  "params": ["0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd"]
}
```

(and the 0x6124fee993bc0000 in the value is 7000000000000000000 == 7 ETH that we wanted to send).

So the last question is -- what is the 'magic' contract address: `0x800a` ?

```solidity
/// @dev The address of the eth token system contract
address constant L2_BASE_TOKEN_SYSTEM_CONTRACT_ADDR = address(0x800a);

```

### System contracts (on L2)

This is a good opportunity to talk about system contracts that are automatically deployed on L2. You can find the full
list here
[in github](https://github.com/matter-labs/era-system-contracts/blob/436d57da2fb35c40e38bcb6637c3a090ddf60701/scripts/constants.ts#L29)

This is the place where we specify that `bootloader` is at address 0x8001, `NonceHolder` at 0x8003 etc.

This brings us to
[L2BaseToken.sol](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/L2EthToken.sol) that has the
implementation of the L2 Eth.

When we look inside, we can see:

```solidity
// Send the L2 log, a user could use it as proof of the withdrawal
bytes memory message = _getL1WithdrawMessage(_l1Receiver, amount);
L1_MESSENGER_CONTRACT.sendToL1(message);
```

And `L1MessengerContract` (that is deployed at 0x8008).

### Committing to L1

And how do these messages get into the L1? The `eth_sender` class from our server is taking care of this. You can see
the details of the transactions that it posts to L1 in our database in `eth_txs` table.

If you look at the `tx_type` column (in psql), you can see that we have 3 different transaction types:

```sql
zksync_local=# select contract_address, tx_type from eth_txs;
              contract_address              |          tx_type
--------------------------------------------+---------------------------
 0x54e8159f006750466084913d5bd288d4afb1ee9a | CommitBlocks
 0x54e8159f006750466084913d5bd288d4afb1ee9a | PublishProofBlocksOnchain
 0x54e8159f006750466084913d5bd288d4afb1ee9a | ExecuteBlocks
 0x54e8159f006750466084913d5bd288d4afb1ee9a | CommitBlocks
 0x54e8159f006750466084913d5bd288d4afb1ee9a | PublishProofBlocksOnchain
 0x54e8159f006750466084913d5bd288d4afb1ee9a | ExecuteBlocks
```

BTW - all the transactions are sent to the 0x54e address - which is the `DiamondProxy` deployed on L1 (this address will
be different on your local node - see previous tutorial for more info) .

And inside, all three methods above belong to
[Executor.sol](https://github.com/matter-labs/era-contracts/blob/dev/l1-contracts/contracts/state-transition/chain-deps/facets/Executor.sol)
facet and you can look at
[README](https://github.com/matter-labs/era-contracts/blob/main/docs/Overview.md#executorfacet) to see the details of
what each method does.

The short description is:

- 'CommitBlocks' - is verifying the block metadata and stores the hash into the L1 contract storage.
- 'PublishProof' - gets the proof, checks that the proof is correct and that it is a proof for the block hash that was
  stored in commit blocks. (IMPORTANT: in testnet/localnet we allow empty proofs - so that you don't have to run the
  full prover locally)
- 'ExecuteBlocks' - is the final call, that stores the root hashes in L1 storage. This allows other calls (like
  finalizeWithdrawal) to work.

So to sum it up - after these 3 calls, the L1 contract has a root hash of a merkle tree, that contains the 'message'
about the withdrawal.

### Final step - finalizing withdrawal

Now we're ready to actually claim our ETH on L1. We do this by calling a `finalizeEthWithdrawal` function on the
DiamondProxy contract (Mailbox.sol to be exact).

To prove that we actually can withdraw the money, we have to say in which L2 block the withdrawal happened, and provide
the merkle proof from our withdrawal log, to the root that is stored in the L1 contract.
