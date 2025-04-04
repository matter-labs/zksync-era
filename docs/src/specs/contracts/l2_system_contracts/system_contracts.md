# System contracts

While most of the primitive EVM opcodes can be supported out of the box (i.e. zero-value calls,
addition/multiplication/memory/storage management, etc), some of the opcodes are not supported by the VM by default and
they are implemented via “system contracts” — these contracts are located in a special _kernel space,_ i.e. in the
address space in range `[0..2^16-1]`, and they have some special privileges, which users’ contracts don’t have. These
contracts are pre-deployed at the genesis and updating their code can be done only via system upgrade, managed from L1.

The use of each system contract will be explained down below.

Most of the details on the implementation and the requirements for the execution of system contracts can be found in the
doc-comments of their respective code bases. This chapter serves only as a high-level overview of such contracts.

All the codes of system contracts (including `DefaultAccount`s) are part of the protocol and can only be change via a
system upgrade through L1.

## SystemContext

This contract is used to support various system parameters not included in the VM by default, i.e. `chainId`, `origin`,
`ergsPrice`, `blockErgsLimit`, `coinbase`, `difficulty`, `baseFee`, `blockhash`, `block.number`, `block.timestamp.`

It is important to note that the constructor is **not** run for system contracts upon genesis, i.e. the constant context
values are set on genesis explicitly. Notably, if in the future we want to upgrade the contracts, we will do it via
[ContractDeployer](#contractdeployer--immutablesimulator) and so the constructor will be run.

This contract is also responsible for ensuring validity and consistency of batches, L2 blocks and virtual blocks. The
implementation itself is rather straightforward, but to better understand this contract, please take a look at the
[page](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/Batches%20&%20L2%20blocks%20on%20zkSync.md)
about the block processing on ZKsync.

## AccountCodeStorage

The code hashes of accounts are stored inside the storage of this contract. Whenever a VM calls a contract with address
`address` it retrieves the value under storage slot `address` of this system contract, if this value is non-zero, it
uses this as the code hash of the account.

Whenever a contract is called, the VM asks the operator to provide the preimage for the codehash of the account. That is
why data availability of the code hashes is paramount.

### Constructing vs Non-constructing code hash

In order to prevent contracts from being able to call a contract during its construction, we set the marker (i.e. second
byte of the bytecode hash of the account) as `1`. This way, the VM will ensure that whenever a contract is called
without the `isConstructor` flag, the bytecode of the default account (i.e. EOA) will be substituted instead of the
original bytecode.

## BootloaderUtilities

This contract contains some of the methods which are needed purely for the bootloader functionality but were moved out
from the bootloader itself for the convenience of not writing this logic in Yul.

## DefaultAccount

Whenever a contract that does **not** both:

- belong to kernel space
- have any code deployed on it (the value stored under the corresponding storage slot in `AccountCodeStorage` is zero)

The code of the default account is used. The main purpose of this contract is to provide EOA-like experience for both
wallet users and contracts that call it, i.e. it should not be distinguishable (apart of spent gas) from EOA accounts on
Ethereum.

## Ecrecover

The implementation of the ecrecover precompile. It is expected to be used frequently, so written in pure yul with a
custom memory layout.

The contract accepts the calldata in the same format as EVM precompile, i.e. the first 32 bytes are the hash, the next
32 bytes are the v, the next 32 bytes are the r, and the last 32 bytes are the s.

It also validates the input by the same rules as the EVM precompile:

- The v should be either 27 or 28,
- The r and s should be less than the curve order.

After that, it makes a precompile call and returns empty bytes if the call failed, and the recovered address otherwise.

## Empty contracts

Some of the contracts are relied upon to have EOA-like behaviour, i.e. they can be always called and get the success
value in return. An example of such address is 0 address. We also require the bootloader to be callable so that the
users could transfer ETH to it.

For these contracts, we insert the `EmptyContract` code upon genesis. It is basically a noop code, which does nothing
and returns `success=1`.

## SHA256 & Keccak256

Note that, unlike Ethereum, keccak256 is a precompile (_not an opcode_) on ZKsync.

These system contracts act as wrappers for their respective crypto precompile implementations. They are expected to be
used frequently, especially keccak256, since Solidity computes storage slots for mapping and dynamic arrays with its
help. That's why we wrote contracts on pure yul with optimizing the short input case.

The system contracts accept the input and transform it into the format that the zk-circuit expects. This way, some of
the work is shifted from the crypto to smart contracts, which are easier to audit and maintain.

Both contracts should apply padding to the input according to their respective specifications, and then make a
precompile call with the padded data. All other hashing work will be done in the zk-circuit. It's important to note that
the crypto part of the precompiles expects to work with padded data. This means that a bug in applying padding may lead
to an unprovable transaction.

## EcAdd & EcMul

These precompiles simulate the behaviour of the EVM's EcAdd and EcMul precompiles and are fully implemented in Yul
without circuit counterparts. You can read more about them
[here](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/Elliptic%20curve%20precompiles.md).

## L2BaseToken & MsgValueSimulator

Unlike Ethereum, zkEVM does not have any notion of any special native token. That’s why we have to simulate operations
with Ether via two contracts: `L2BaseToken` & `MsgValueSimulator`.

`L2BaseToken` is a contract that holds the balances of ETH for the users. This contract does NOT provide ERC20
interface. The only method for transferring Ether is `transferFromTo`. It permits only some system contracts to transfer
on behalf of users. This is needed to ensure that the interface is as close to Ethereum as possible, i.e. the only way
to transfer ETH is by doing a call to a contract with some `msg.value`. This is what `MsgValueSimulator` system contract
is for.

Whenever anyone wants to do a non-zero value call, they need to call `MsgValueSimulator` with:

- The calldata for the call equal to the original one.
- Pass `value` and whether the call should be marked with `isSystem` in the first extra abi params.
- Pass the address of the callee in the second extraAbiParam.

More information on the extraAbiParams can be read
[here](../../../guides/advanced/12_alternative_vm_intro.md#flags-for-calls).

## KnownCodeStorage

This contract is used to store whether a certain code hash is “known”, i.e. can be used to deploy contracts. On ZKsync,
the L2 stores the contract’s code _hashes_ and not the codes themselves. Therefore, it must be part of the protocol to
ensure that no contract with unknown bytecode (i.e. hash with an unknown preimage) is ever deployed.

The factory dependencies field provided by the user for each transaction contains the list of the contract’s bytecode
hashes to be marked as known. We can not simply trust the operator to “know” these bytecodehashes as the operator might
be malicious and hide the preimage. We ensure the availability of the bytecode in the following way:

- If the transaction comes from L1, i.e. all its factory dependencies have already been published on L1, we can simply
  mark these dependencies as “known”.
- If the transaction comes from L2, i.e. (the factory dependencies are yet to publish on L1), we make the user pays by
  burning ergs proportional to the bytecode’s length. After that, we send the L2→L1 log with the bytecode hash of the
  contract. It is the responsibility of the L1 contracts to verify that the corresponding bytecode hash has been
  published on L1.

It is the responsibility of the [ContractDeployer](#contractdeployer--immutablesimulator) system contract to deploy only
those code hashes that are known.

The KnownCodesStorage contract is also responsible for ensuring that all the “known” bytecode hashes are also
[valid](../../../guides/advanced/12_alternative_vm_intro.md#bytecode-validity).

## ContractDeployer & ImmutableSimulator

`ContractDeployer` is a system contract responsible for deploying contracts on ZKsync. It is better to understand how it
works in the context of how the contract deployment works on ZKsync. Unlike Ethereum, where `create`/`create2` are
opcodes, on ZKsync these are implemented by the compiler via calls to the ContractDeployer system contract.

For additional security, we also distinguish the deployment of normal contracts and accounts. That’s why the main
methods that will be used by the user are `create`, `create2`, `createAccount`, `create2Account`, which simulate the
CREATE-like and CREATE2-like behavior for deploying normal and account contracts respectively.

### **Address derivation**

Each rollup that supports L1→L2 communications needs to make sure that the addresses of contracts on L1 and L2 do not
overlap during such communication (otherwise it would be possible that some evil proxy on L1 could mutate the state of
the L2 contract). Generally, rollups solve this issue in two ways:

- XOR/ADD some kind of constant to addresses during L1→L2 communication. That’s how rollups closer to full
  EVM-equivalence solve it, since it allows them to maintain the same derivation rules on L1 at the expense of contract
  accounts on L1 having to redeploy on L2.
- Have different derivation rules from Ethereum. That is the path that ZKsync has chosen, mainly because since we have
  different bytecode than on EVM, CREATE2 address derivation would be different in practice anyway.

You can see the rules for our address derivation in `getNewAddressCreate2`/ `getNewAddressCreate` methods in the
ContractDeployer.

Note, that we still add a certain constant to the addresses during L1→L2 communication in order to allow ourselves some
way to support EVM bytecodes in the future.

### **Deployment nonce**

On Ethereum, the same nonce is used for CREATE for accounts and EOA wallets. On ZKsync this is not the case, we use a
separate nonce called “deploymentNonce” to track the nonces for accounts. This was done mostly for consistency with
custom accounts and for having multicalls feature in the future.

### **General process of deployment**

- After incrementing the deployment nonce, the contract deployer must ensure that the bytecode that is being deployed is
  available.
- After that, it puts the bytecode hash with a
  [special constructing marker](#constructing-vs-non-constructing-code-hash) as code for the address of the
  to-be-deployed contract.
- Then, if there is any value passed with the call, the contract deployer passes it to the deployed account and sets the
  `msg.value` for the next as equal to this value.
- Then, it uses `mimic_call` for calling the constructor of the contract out of the name of the account.
- It parses the array of immutables returned by the constructor (we’ll talk about immutables in more details later).
- Calls `ImmutableSimulator` to set the immutables that are to be used for the deployed contract.

Note how it is different from the EVM approach: on EVM when the contract is deployed, it executes the initCode and
returns the deployedCode. On ZKsync, contracts only have the deployed code and can set immutables as storage variables
returned by the constructor.

### **Constructor**

On Ethereum, the constructor is only part of the initCode that gets executed during the deployment of the contract and
returns the deployment code of the contract. On ZKsync, there is no separation between deployed code and constructor
code. The constructor is always a part of the deployment code of the contract. In order to protect it from being called,
the compiler-generated contracts invoke constructor only if the `isConstructor` flag provided (it is only available for
the system contracts). You can read more about flags
[here](../../../guides/advanced/12_alternative_vm_intro.md#flags-for-calls).

After execution, the constructor must return an array of:

```solidity
struct ImmutableData {
  uint256 index;
  bytes32 value;
}

```

basically denoting an array of immutables passed to the contract.

### **Immutables**

Immutables are stored in the `ImmutableSimulator` system contract. The way how `index` of each immutable is defined is
part of the compiler specification. This contract treats it simply as mapping from index to value for each particular
address.

Whenever a contract needs to access a value of some immutable, they call the
`ImmutableSimulator.getImmutable(getCodeAddress(), index)`. Note that on ZKsync it is possible to get the current
execution address you can read more about `getCodeAddress()`
[here](../../../guides/advanced/12_alternative_vm_intro.md#zkevm-specific-opcodes).

### **Return value of the deployment methods**

If the call succeeded, the address of the deployed contract is returned. If the deploy fails, the error bubbles up.

## DefaultAccount

The implementation of the default account abstraction. This is the code that is used by default for all addresses that
are not in kernel space and have no contract deployed on them. This address:

- Contains minimal implementation of our account abstraction protocol. Note that it supports the
  [built-in paymaster flows](https://v2-docs.zksync.io/dev/developer-guides/aa.html#paymasters).
- When anyone (except bootloader) calls it, it behaves in the same way as a call to an EOA, i.e. it always returns
  `success = 1, returndatasize = 0` for calls from anyone except for the bootloader.

## L1Messenger

A contract used for sending arbitrary length L2→L1 messages from ZKsync to L1. While ZKsync natively supports a rather
limited number of L1→L2 logs, which can transfer only roughly 64 bytes of data a time, we allowed sending
nearly-arbitrary length L2→L1 messages with the following trick:

The L1 messenger receives a message, hashes it and sends only its hash as well as the original sender via L2→L1 log.
Then, it is the duty of the L1 smart contracts to make sure that the operator has provided full preimage of this hash in
the commitment of the batch.

The `L1Messenger` is also responsible for validating the total pubdata to be sent on L1. You can read more about it
[here](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/Handling%20pubdata%20in%20Boojum.md).

## NonceHolder

Serves as storage for nonces for our accounts. Besides making it easier for operator to order transactions (i.e. by
reading the current nonces of account), it also serves a separate purpose: making sure that the pair (address, nonce) is
always unique.

It provides a function `validateNonceUsage` which the bootloader uses to check whether the nonce has been used for a
certain account or not. Bootloader enforces that the nonce is marked as non-used before validation step of the
transaction and marked as used one afterwards. The contract ensures that once marked as used, the nonce can not be set
back to the “unused” state.

Note that nonces do not necessarily have to be monotonic (this is needed to support more interesting applications of
account abstractions, e.g. protocols that can start transactions on their own, tornado-cash like protocols, etc). That’s
why there are two ways to set a certain nonce as “used”:

- By incrementing the `minNonce` for the account (thus making all nonces that are lower than `minNonce` as used).
- By setting some non-zero value under the nonce via `setValueUnderNonce`. This way, this key will be marked as used and
  will no longer be allowed to be used as nonce for accounts. This way it is also rather efficient, since these 32 bytes
  could be used to store some valuable information.

The accounts upon creation can also provide which type of nonce ordering do they want: Sequential (i.e. it should be
expected that the nonces grow one by one, just like EOA) or Arbitrary, the nonces may have any values. This ordering is
not enforced in any way by system contracts, but it is more of a suggestion to the operator on how it should order the
transactions in the mempool.

## EventWriter

A system contract responsible for emitting events.

It accepts in its 0-th extra abi data param the number of topics. In the rest of the extraAbiParams he accepts topics
for the event to emit. Note, that in reality the event the first topic of the event contains the address of the account.
Generally, the users should not interact with this contract directly, but only through Solidity syntax of `emit`-ing new
events.

## Compressor

One of the most expensive resource for a rollup is data availability, so in order to reduce costs for the users we
compress the published pubdata in several ways:

- We compress published bytecodes.
- We compress state diffs.

This contract contains utility methods that are used to verify the correctness of either bytecode or state diff
compression. You can read more on how we compress state diffs and bytecodes in the corresponding
[document](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/Handling%20L1%E2%86%92L2%20ops%20on%20zkSync.md).

## Known issues to be resolved

The protocol, while conceptually complete, contains some known issues which will be resolved in the short to middle
term.

- Fee modeling is yet to be improved. More on it in the
  [document](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/zkSync%20fee%20model.md)
  on the fee model.
- We may add some kind of default implementation for the contracts in the kernel space (i.e. if called, they wouldn’t
  revert but behave like an EOA).
