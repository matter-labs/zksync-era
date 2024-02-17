# zkEVM 内部机制

## zkEVM 澄清器

[返回目录](../../specs/README.md)

在 zkStack 中，zkSync 的 zkEVM 在功能上与以太坊中的 EVM 扮演着根本不同的角色。EVM 用于执行以太坊的状态转换函数。这个状态转换函数需要客户端来实现和运行。以太坊有一个多客户端的哲学，有多个客户端，它们是用 Go、Rust 和其他传统编程语言编写的，都在运行和验证相同的状态转换函数。

我们有一套不同的要求，我们需要证明某个客户端正确执行了状态转换函数。第一个后果是客户端需要被硬编码，我们不能采用相同的多客户端哲学。这个客户端就是 zkEVM，它可以高效地运行状态转换函数，包括类似于 EVM 的智能合约的执行。zkEVM 也被设计为能够高效地进行证明。

出于效率的考虑，zkEVM 与 EVM 类似。这使得在其中执行智能程序变得容易。它还具有一些特殊功能，在 EVM 中没有但在滚动更新的状态转换函数、存储、燃气计量、预编译和其他方面是必需的。其中一些功能被实现为系统合约，而其他一些则内置于 VM 中。系统合约是具有特殊权限的合约，部署在预定义的地址上。最后，我们有引导加载程序，它也是一个合约，尽管它没有部署在任何地址上。这是最终由 zkEVM 执行的状态转换函数，并对状态执行事务。

完整的 zkEVM 规范超出了本文档的范围。然而，本节将为您提供了解 L2 系统智能合约和 EVM 与 zkEVM 之间基本区别所需的大部分细节。还请注意，通常需要了解 EVM 才能进行高效的智能合约开发。了解 zkEVM 不仅仅是这个，它对于开发滚动更新本身是必要的。

## 寄存器和内存管理

在 EVM 上，在事务执行期间，以下内存区域是可用的：

- `memory` 本身。
- `calldata` 父内存的不可变片段。
- `returndata` 最新调用另一个合约时返回的不可变片段。
- `stack` 本地变量存储的地方。

与 EVM 不同，它是栈机器，zkEVM 有 16 个寄存器。zkEVM 不是从 `calldata` 接收输入，而是从父级内存的 `calldata` 页开始，并在第一个寄存器中接收一个指针（基本上是一个包含 4 个元素的紧凑结构：内存页 ID，指向的切片的起始和长度）。同样，事务可以在程序开始时在其寄存器中接收一些其他附加数据：是否应该调用构造函数的事务[这里有更多关于部署的信息](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#contractdeployer--immutablesimulator)，事务是否具有 `isSystem` 标志等。这些标志的每个含义将在本节后续展开。

指针在 VM 中是一个单独的类型。只有可能：

- 在指针内读取某个值。
- 通过减少指针指向的切片来缩小指针。
- 将指向 `returndata` 的指针作为 calldata 接收。
- 指针只能存储在栈/寄存器上，以确保其他合约不能读取它们不应该读取的合约的 `memory/returndata`。
- 可以将指针转换为表示它的 u256 整数，但整数不能转换为指针，以防止不允许的内存访问。
- 不可能返回指向 ID 小于当前页的内存页的指针。这意味着只能将指向当前帧内存或当前帧子调用返回的指针“返回”。

### zkEVM 中的内存区域

对于每个帧，分配以下内存区域：

- _Heap_（在以太坊上扮演与 `memory` 相同的角色）。
- _AuxHeap_（辅助堆）。它具有与 Heap 相同的属性，但它用于编译器对 calldata/copy 进行编码，以不干扰标准的 Solidity 内存对齐。
- _Stack_。与以太坊不同，栈不是获取操作码参数的主要位置。zkSync 栈与 EVM 的最大差异在于，在 zkSync 上，栈可以在任何位置访问（就像内存一样）。虽然用户不需要为栈的增长付费，但栈可以在帧结束时完全清除，因此开销很小。
- _Code_。VM 执行合约代码的内存区域。合约本身无法读取代码页，这只是 VM 隐式完成的。

此外，如前一节所述，合约接收指向 calldata 的指针。

### 管理 returndata 和 calldata

每当合约完成其执行时，父帧都会收到一个指针作为 `returndata`。该指针可能指向子帧的 Heap/AuxHeap，或者甚至可以是子帧从其某些子帧收到的相同 `returndata` 指

针。

对于 `calldata` 也是一样。每当合约开始执行时，它都会收到指向 calldata 的指针。父帧可以提供任何有效的指针作为 calldata，这意味着它可以是指向父帧内存（heap 或 auxHeap）的切片的指针，或者它可以是父帧之前作为 calldata/returndata 接收的一些有效指针。

合约只是在执行帧的开始处记住 calldata 指针（这是编译器设计的），并记住最新接收的 returndata 指针。

这样做的一些重要含义是，现在可以进行以下调用而无需进行任何内存复制：

A → B → C

其中 C 接收了 B 接收的 calldata 切片。

对于返回数据也是一样：

A ← B ← C

如果 B 返回 C 返回的 returndata 切片，则无需复制返回的数据。

请注意，不能使用通过 calldata 接收的指针作为 returndata（即在执行帧结束时返回它）。否则，returndata 可能指向活动帧的内存切片并允许编辑 returndata。这意味着在上述示例中，C 不能返回其 calldata 的切片而无需内存复制。

这些内存优化中的一些可以在 [EfficientCall](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/EfficientCall.sol) 库中看到。

### Returndata 和预编译

一些在以太坊上是操作码的操作已经变成了对一些系统合约的调用。最明显的例子是 `Keccak256`、`SystemContext` 等。请注意，如果天真地执行，下面的代码行将在 zkSync 和以太坊上有不同的工作方式：

```solidity
pop(call(...))
keccak(...)
returndatacopy(...)
```

由于对 keccak 预编译的调用将修改 `returndata`。为了避免这种情况，我们的编译器不会在调用此类操作码式预编译后覆盖最新的 `returndata` 指针。

## zkEVM 特定操作码

虽然某些以太坊操作码不是直接支持的，但一些新操作码已经添加以便于系统合约的开发。

请注意，这个列表并不旨在具体说明内部机制，而是解释[SystemContractHelper.sol](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/SystemContractHelper.sol)中的方法。

### **仅限于内核空间**

这些操作码仅允许在内核空间的合约中（即系统合约）使用。如果在其他地方执行，会导致 `revert(0,0)`。

- `mimic_call`。与普通的 `call` 相同，但它可以更改事务的 `msg.sender` 字段。
- `to_l1`。向以太坊发送系统 L2→L1 日志。可以在[这里](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/contracts/ethereum/contracts/zksync/Storage.sol#L47)看到此日志的结构。
- `event`。向 zkSync 发出 L2 日志。请注意，L2 日志与以太坊事件不等效。每个 L2 日志可以发出 64 字节的数据（实际大小为 88 字节，因为它包含发出者地址等）。单个以太坊事件由多个 `event` 日志组成。此操作码仅由 `EventWriter` 系统合约使用。
- `precompile_call`。这是一个接受两个参数的操作码：表示其紧凑参数的 uint256，以及要燃烧的 ergs。除了预编译调用本身的价格之外，它还会燃烧提供的 ergs 并执行预编译。它在执行期间取决于 `this` 所做的操作：
  - 如果它是 `ecrecover` 系统合约的地址，则执行 ecrecover 操作
  - 如果它是 `sha256`/`keccak256` 系统合约的地址，则执行相应的哈希操作。
  - 否则什么都不做（即仅燃烧 ergs）。它可用于燃烧 L2→L1 通信所需的 ergs，或者在链上发布字节码。
- `setValueForNextFarCall`。为下一个 `call`/`mimic_call` 设置 `msg.value`。请注意，这并不意味着值会真正被转移。它只是设置相应的 `msg.value` 上下文变量。使用此参数的系统合约应通过其他方式执行 ETH 的转移。请注意，此方法不会对 `delegatecall` 产生影响，因为 `delegatecall` 继承了上一个帧的 `msg.value`。
- `increment_tx_counter`。增加 VM 内的事务计数器。事务计数器主要用于 VM 内部跟踪事件。仅在引导加载程序中的每个事务结束后使用。

请注意，目前我们无法在 VM 内部访问 `tx_counter`（即使现在可以增加它，并且将自动用于诸如 `event` 之类的日志以及由 `to_l1` 产生的系统日志，但我们无法读取它）。我们需要读取它来发布 _用户_ L2→L1 日志，因此 `increment_tx_counter` 始终与[SystemContext](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_e

vm/system_contracts.md#systemcontext)合约的相应调用一起使用。

有关系统和用户日志之间的区别的更多信息，请阅读[此处](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/Handling%20pubdata%20in%20Boojum.md)。- `set_pubdata_price` 设置发布单个 pubdata 字节的价格（以 gas 计）。

### **通常可访问的**

以下是任何合约都可以访问的操作码。请注意，虽然 VM 允许访问这些方法，但这并不意味着这很容易：编译器可能尚未为某些用例提供方便的支持。

- `near_call`。基本上是对合约代码的某个位置进行“帧”跳转。 `near_call` 和普通跳转之间的区别是：
  1. 可以为其提供一个 ergsLimit。请注意，与“`far_call`”（即合约之间的调用）不同，它们不适用于 63/64 规则。
  2. 如果 near 调用帧出现异常，则由其进行的所有状态更改都将被撤消。请注意，内存更改**不会**被撤消。
- `getMeta`。返回[ZkSyncMeta](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/libraries/SystemContractHelper.sol#L42)结构的 u256 打包值。请注意，这不是紧密打包。该结构由以下 rust 代码形成。
- `getCodeAddress`。接收执行代码的地址。这与 `this` 不同，因为在委托调用的情况下，`this` 保留，但 `codeAddress` 不保留。

### 调用标志

除了 calldata，还可以在执行 `call`、`mimic_call`、`delegate_call` 时为被调用方提供附加信息。调用的合约将在执行开始时在其前 12 个寄存器中接收以下信息：

- _r1_ — 指向 calldata 的指针。
- _r2_ — 具有调用标志的指针。这是一个掩码，其中每个位仅在调用时设置了某些标志时才设置。当前支持两个标志：第 0 位：`isConstructor` 标志。此标志只能由系统合约设置，并表示帐户是否应执行其构造函数逻辑。请注意，与以太坊不同，没有构造函数和部署字节码之间的区分。可以在[这里](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#contractdeployer--immutablesimulator)阅读更多内容。
  1. 第 1 位：`isSystem` 标志。调用是否意图调用系统合约的函数。虽然大多数系统合约的功能相对无害，但仅使用 calldata 访问一些可能会破坏以太坊不变性的操作可能会有问题，例如，如果系统合约使用 `mimic_call`：没有人预期通过调用合约可能会以调用方的名义执行某些操作。如果被调用者位于内核空间，则仅可以设置此标志。
- 其余 r3..r12 寄存器仅在设置了 `isSystem` 标志时才有非空值。可能会传递任意值，我们称之为 `extraAbiParams`。

编译器的实现是合约会记住这些标志，并且可以在执行期间通过特殊的[simulations](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/overview.md)访问它们。

如果调用方提供了不适当的标志（即尝试在被调用者不在内核空间时设置 `isSystem` 标志），则会忽略这些标志。

### `onlySystemCall` 修饰符

一些系统合约可以代表用户行事，或者对帐户的行为产生非常重要的影响。这就是为什么我们想要明确指出用户不能通过简单的类似于 EVM 的 `call` 来调用可能危险的操作。每当用户想要调用我们认为危险的某些操作时，他们必须提供“`isSystem`”标志。

`onlySystemCall` 标志检查是否提供了“isSystemCall”标志或者调用是由另一个系统合约执行的（因为 Matter Labs 完全了解系统合约）。

### 通过我们的编译器进行模拟

在将来，我们计划引入我们的“扩展”版本的 Solidity，支持的操作码比原始版本更多。但是，目前团队的能力不足以做到这一点，因此为了表示访问 zkSync 特定操作码，我们使用带有特定常量参数的 `call` 操作码，这些参数将由编译器自动替换为 zkEVM 本机操作码。

示例：

```solidity
function getCodeAddress() internal view returns (address addr) {
  address callAddr = CODE_ADDRESS_CALL_ADDRESS;
  assembly {
    addr := staticcall(0, callAddr, 0, 0xFFFF, 0, 0)
  }
}

```

在上面的示例中，编译器将检测到静态调用是针对常量 `CODE_ADDRESS_CALL_ADDRESS` 进行的，因此它将使用获取当前执行的代码地址的操作码替换它。

操作码模拟的完整列表可以在[此处](https://github.com/code-423

n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/overview.md)找到。

### 总结

zkEVM 是 zkSync 的核心组件之一，它扮演着与以太坊 EVM 不同的角色。它需要与以太坊智能合约开发和 EVM 机制相关的知识，但也有一些独特的特性和操作码，例如针对系统合约的特殊调用标志、内存优化以及对 zkSync 智能合约的支持。理解 zkEVM 对于开发滚动更新和在 zkSync 上构建安全、高效的智能合约至关重要。