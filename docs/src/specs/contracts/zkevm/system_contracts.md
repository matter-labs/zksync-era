# System contracts

While most of the primitive EVM opcodes can be supported out of the box (i.e. zero-value calls,
addition/multiplication/memory/storage management, etc), some of the opcodes are not supported by the VM by default and
they are implemented via “system contracts” — these contracts are located in a special _kernel space_, i.e. in the
address space in the range `[0..2^16-1]`, and they have some special privileges, which users’ contracts don’t have. These
contracts are pre-deployed at genesis and updating their code can be done only via system upgrade, managed from L1.

The use of each system contract will be explained down below.

Most of the details on the implementation and the requirements for the execution of system contracts can be found in the
doc-comments of their respective code bases. This chapter serves only as a high-level overview of such contracts.

All of the system contracts’ code (including `DefaultAccount`s) are part of the protocol and can only be changed via a
system upgrade through L1.

## SystemContext

This contract is used to support various system parameters not included in the VM by default, i.e. `chainId`, `origin`,
`ergsPrice`, `blockErgsLimit`, `coinbase`, `difficulty`, `baseFee`, `blockhash`, `block.number`, `block.timestamp.`

It is important to note that the constructor is **not** run for system contracts upon genesis, i.e. the constant context
values are set on genesis explicitly. Notably, if in the future we want to upgrade the contracts, we will do it via
[ContractDeployer](#contractdeployer--immutablesimulator) and so the constructor will be run.

This contract is also responsible for ensuring validity and consistency of batches, L2 blocks and virtual blocks. The
implementation itself is rather straightforward, but to better understand this contract, please take a look at the
[page](./batches_and_blocks_on_zksync.md)
about the block processing on ZKsync.

## AccountCodeStorage

The code hashes of accounts are stored inside the storage of this contract. Whenever a VM calls a contract with address
`address`, it retrieves the value under storage slot `address` of this system contract; if this value is non-zero, it
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

Whenever a contract does not both belong to kernel space and have any code deployed on it (the value stored under the
corresponding storage slot in `AccountCodeStorage` is zero), the code of the default account is used. The main purpose
of this contract is to provide an EOA-like experience for both wallet users and contracts that call it, i.e. it should
not be distinguishable (apart from spent gas) from EOA accounts on Ethereum.

## Ecrecover

The implementation of the ecrecover precompile. It is expected to be used frequently, so written in pure Yul with a
custom memory layout.

The contract accepts the calldata in the same format as the EVM precompile, i.e. the first 32 bytes are the hash, the
next 32 bytes are the v, the next 32 bytes are the r, and the last 32 bytes are the s.

It also validates the input by the same rules as the EVM precompile:

- The v should be either 27 or 28,
- The r and s should be less than the curve order.

After that, it makes a precompile call and returns empty bytes if the call failed, and the recovered address otherwise.

## Empty contracts

Some of the contracts are relied upon to have EOA-like behavior, i.e. they can always be called and get the success
value in return. An example of such an address is the 0 address. We also require the bootloader to be callable so that
the users could transfer ETH to it.

For these contracts, we insert the `EmptyContract` code upon genesis. It is basically a no-op code, which does nothing
and returns `success=1`.

## SHA256 & Keccak256

Note that, unlike Ethereum, keccak256 is a precompile (_not an opcode_) on ZKsync.

These system contracts act as wrappers for their respective crypto precompile implementations. They are expected to be
used frequently, especially keccak256, since Solidity computes storage slots for mapping and dynamic arrays with its
help. That’s why we wrote contracts in pure Yul optimized for the short-input case.

The system contracts accept the input and transform it into the format that the zk-circuit expects. This way, some of
the work is shifted from the crypto to smart contracts, which are easier to audit and maintain.

Both contracts should apply padding to the input according to their respective specifications, and then make a
precompile call with the padded data. All other hashing work will be done in the zk-circuit. It's important to note that
the crypto part of the precompiles expects to work with padded data. This means that a bug in applying padding may lead
to an unprovable transaction.

## EcAdd & EcMul

These precompiles simulate the behaviour of the EVM's EcAdd and EcMul precompiles and are fully implemented in Yul
without circuit counterparts. You can read more about them
[here](./precompiles.md).

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

## Support for `.send/.transfer`

On Ethereum, whenever a call with non-zero value is done, some additional gas is charged from the caller's frame and in return a `2300` gas stipend is given out to the callee frame. This stipend is usually enough to emit a small event, but it is enforced that it is not possible to change storage within these `2300` gas. This also means that in practice some users might opt to do `call` with 0 gas provided, relying on the `2300` stipend to be passed to the callee. This is the case for `.call/.transfer`.

While using `.send/.transfer` is generally not recommended, as a step towards better EVM compatibility, since vm1.5.0 a _partial_ support of these functions is present with ZKsync Era. It is the done via the following means:

- Whenever a call is done to the `MsgValueSimulator` system contract, `27000` gas is deducted from the caller's frame and it passed to the `MsgValueSimulator` on top of whatever gas the user has originally provided. The number was chosen to cover for the execution of the transferring of the balances as well as other constant size operations by the `MsgValueSimulator`. Note, that since it will be the frame of `MsgValueSimulator` that will actually call the callee, the constant must also include the cost for decommitting the code of the callee. Decoding bytecode of any size would be prohibitevely expensive and so we support only callees of size up to `100000` bytes.
- `MsgValueSimulator` ensures that no more than `2300` out of the stipend above gets to the callee, ensuring the reentrancy protection invariant for these functions holds.

Note that, unlike EVM, any unused gas from such calls will be refunded.

The system preserves the following guarantees about `.send/.transfer`:

- No more than `2300` gas will be received by the callee. Note that [a smaller, but a close amount](https://github.com/matter-labs/era-contracts/blob/main/system-contracts/contracts/test-contracts/TransferTest.sol#L33) may be passed.
- It is not possible to do any storage changes within this stipend. This is enforced by having cold writes cost more than `2300` gas. Also, cold writes always have to be prepaid whenever executing storage writes.
- Any callee with bytecode size of up to `100000` will work.

The system does not guarantee the following:

- That callees with bytecode size larger than `100000` will work. Note, that a malicious operator can fail any call to a callee with large bytecode even if it has been decommitted before.

As a conclusion, using `.send/.transfer` should be generally avoided, but when avoiding is not possible it should be used with small callees, e.g. EOAs, which implement `DefaultAccount`.

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

You can see the rules for our address derivation in `getNewAddressCreate2`/`getNewAddressCreate` methods in the
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
the compiler-generated contracts invoke the constructor only if the `isConstructor` flag is provided (it is only
available for the system contracts). You can read more about flags
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

Immutables are stored in the `ImmutableSimulator` system contract. The way the `index` of each immutable is defined is
part of the compiler specification. This contract treats it simply as a mapping from index to value for each particular
address.

Whenever a contract needs to access a value of some immutable, they call the
`ImmutableSimulator.getImmutable(getCodeAddress(), index)`. Note that on ZKsync it is possible to get the current
execution address. You can read more about `getCodeAddress()`
[here](../../../guides/advanced/12_alternative_vm_intro.md#zkevm-specific-opcodes).

### **Return value of the deployment methods**

If the call succeeded, the address of the deployed contract is returned. If the deploy fails, the error bubbles up.

## DefaultAccount

The implementation of the default account abstraction. This is the code that is used by default for all addresses that
are not in kernel space and have no contract deployed on them. This address:

- Contains a minimal implementation of our account abstraction protocol. Note that it supports the
  [built-in paymaster flows](https://v2-docs.zksync.io/dev/developer-guides/aa.html#paymasters).
- When anyone (except bootloader) calls it, it behaves in the same way as a call to an EOA, i.e. it always returns
  `success = 1, returndatasize = 0` for calls from anyone except for the bootloader.

## L1Messenger

A contract used for sending arbitrary-length L2→L1 messages from ZKsync to L1. While ZKsync natively supports a rather
limited number of L1→L2 logs, which can transfer only roughly 64 bytes of data at a time, we allow sending nearly-
arbitrary-length L2→L1 messages with the following trick:

The L1 messenger receives a message, hashes it and sends only its hash as well as the original sender via L2→L1 log.
Then, it is the duty of the L1 smart contracts to make sure that the operator has provided full preimage of this hash in
the commitment of the batch.

The `L1Messenger` is also responsible for validating the total pubdata to be sent on L1. You can read more about it
[here](../settlement_contracts/data_availability/pubdata.md).

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
will no longer be allowed to be used as a nonce for accounts. This way it is also rather efficient, since these 32
bytes could be used to store some valuable information.

The accounts, upon creation, can also specify which type of nonce ordering they want: Sequential (i.e. it should be
expected that the nonces grow one by one, just like EOA) or Arbitrary, where the nonces may have any values. This ordering
is not enforced in any way by system contracts, but it is more of a suggestion to the operator on how they should order
the transactions in the mempool.

## EventWriter

A system contract responsible for emitting events.

It accepts in its 0-th extra ABI data param the number of topics. In the rest of the extra ABI params, it accepts topics
for the event to emit. Note that, in reality, the first topic of the event contains the address of the account. Generally,
the users should not interact with this contract directly, but only through Solidity syntax of `emit`-ing new events.

## Compressor

One of the most expensive resources for a rollup is data availability, so in order to reduce costs for the users we
compress the published pubdata in several ways:

- We compress published bytecodes.
- We compress state diffs.

This contract contains utility methods that are used to verify the correctness of either bytecode or state diff
compression. You can read more on how we compress [state diffs](../settlement_contracts/data_availability/compression.md) and [bytecodes](../../../guides/advanced/11_compression.md).
### Pubdata Chunk Publisher

This contract is responsible for separating pubdata into chunks that each fit into a [4844 blob](../settlement_contracts/data_availability/rollup_da.md) and calculating the hash of the preimage of said blob. If a chunk's size is less than the total number of bytes for a blob, we pad it on the right with zeroes, since the circuits require the chunk to be of the exact size.

This contract can be used by L2DAValidators, e.g. [RollupL2DAValidator](https://github.com/matter-labs/era-contracts/blob/main/l2-contracts/contracts/data-availability/RollupL2DAValidator.sol) uses it to compress the pubdata into blobs.

### CodeOracle

It is a contract that accepts a versioned hash of bytecode and returns its preimage. It is similar to the `extcodecopy` functionality on Ethereum.

It works as follows:

1. It accepts a versioned hash and double checks that it is marked as “known”, i.e. the operator must know the preimage for such hash.
2. After that, it uses the `decommit` opcode, which accepts the versioned hash and the number of ergs to spend, which is proportional to the length of the preimage. If the preimage has been decommitted before, the requested cost will be refunded to the user.

 Note, that the decommitment process does not only happen using the `decommit` opcode, but during calls to contracts. Whenever a contract is called, its code is decommitted into a memory page dedicated to contract code. We never decommit the same preimage twice, regardless of whether it was decommitted via an explicit opcode or during a call to another contract, the previous unpacked bytecode memory page will be reused. When executing `decommit` inside the `CodeOracle` contract, the user will be first precharged with the maximum possible cost and then it will be refunded in case the bytecode has been decommitted before.

3. The `decommit` opcode returns a slice of the decommitted bytecode. Note, that the returned pointer always has length of 2^21 bytes, regardless of the length of the actual bytecode. So it is the job of the `CodeOracle` system contract to shrink the length of the returned data.

### P256Verify

This contract exerts the same behavior as the P256Verify precompile from [RIP-7212](https://github.com/ethereum/RIPs/blob/master/RIPS/rip-7212.md). Note, that since Era has different gas schedule, we do not comply with the gas costs, but otherwise the interface is identical.

### GasBoundCaller

This is not a system contract, but it will be predeployed on a fixed user space address. This contract allows users to set an upper bound for how much pubdata a subcall can take, regardless of the gas per pubdata. You can read more about how pubdata works on ZKsync [here](./zksync_fee_model.md).

Note, that it is a deliberate decision not to deploy this contract in the kernel space, since it can relay calls to any contracts and so may break the assumption that all system contracts can be trusted.

### ComplexUpgrader

Usually an upgrade is performed by calling the `forceDeployOnAddresses` function of ContractDeployer out of the name of the `FORCE_DEPLOYER` constant address. However some upgrades may require more complex interactions, e.g. query something from a contract to determine which calls to make etc.

For cases like this `ComplexUpgrader` contract has been created. The assumption is that the implementation of the upgrade is predeployed and the `ComplexUpgrader` will delegatecall to it.

> Note, that while `ComplexUpgrader` existed even in the previous upgrade, it lacked the `forceDeployAndUpgrade` function. This caused some serious limitations. You can read more about how the gateway upgrade process will work [here](../../upgrade_history/gateway_upgrade/upgrade_process_no_gateway_chain.md).

### Create2Factory

Just a built-in Create2Factory. It allows to deterministically deploy contracts to the same address on multiple chains.

### L2GenesisUpgrade

A contract that is responsible for facilitating initialization of a newly created chain. This is part of a [chain creation flow](../chain_management/chain_genesis.md).

### Bridging-related contracts

`L2Bridgehub`, `L2AssetRouter`, `L2NativeTokenVault`, as well as `L2MessageRoot`.

These contracts are used to facilitate cross-chain communication as well as value bridging. You can read more about them in [the asset router spec](../bridging/asset_router_and_ntv/asset_router.md).

Note, that [L2AssetRouter](https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/contracts/bridge/asset-router/L2AssetRouter.sol) and [L2NativeTokenVault](https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/contracts/bridge/ntv/L2NativeTokenVault.sol) have unique code, however the L2Bridgehub and L2MessageRoot share the same source code with their L1 precompiles, i.e. the L2Bridgehub has [this](https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/contracts/bridgehub/Bridgehub.sol) code and L2MessageRoot has [this](https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/contracts/bridgehub/MessageRoot.sol) code.

### SloadContract

During the L2GatewayUpgrade, the system contracts will need to read the storage of some other contracts, despite the fact that they lack getters.. How it is implemented can be read in the `forcedSload` function of the [SystemContractHelper](https://github.com/matter-labs/era-contracts/blob/main/system-contracts/contracts/libraries/SystemContractHelper.sol) contract.

While it is only used for the upgrade, it was decided to leave it as a predeployed contract for future use-cases as well.

### L2WrappedBaseTokenImplementation

Although bridging wrapped base tokens (e.g., WETH) is not yet supported, its address is enshrined within the native token vault (both the L1 and L2 ones). For consistency with other networks, our WETH token is deployed as a TransparentUpgradeableProxy. To make the deployment process easier, we predeploy the implementation.

## Known issues to be resolved

The protocol, while conceptually complete, contains some known issues which will be resolved in the short to middle
term.

- Fee modeling is yet to be improved. More on it in the
  [document](./zksync_fee_model.md)
  on the fee model.
- We may add some kind of default implementation for the contracts in the kernel space (i.e. if called, they wouldn’t
revert but behave like an EOA).
