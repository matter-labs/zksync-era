# Exception Handling

This document explains some peculiarities of the exception handling (EH) in zkEVM architecture.

In a nutshell, there are two exception handling mechanisms in zkEVM: contract-level and function-level. The former is
more common to general-purpose languages, and the latter was inherited from the EVM architecture.

|                                                        | Contract Level          | Function Level                                 |
| ------------------------------------------------------ | ----------------------- | ---------------------------------------------- |
| Yul examples                                           | revert(0, 0)            | verbatim("throw")                              |
| Native to                                              | EVM                     | General-purpose languages                      |
| Handled by                                             | zkEVM                   | Compiler                                       |
| Catchable                                              | By the calling contract | By the calling function                        |
| Efficient                                              | Yes                     | Huge size impact due to numerous catch blocks. |
| Extra cycles are needed for propagating the exception. |

## Contract Level

This type of exceptions is inherited from the EVM architecture. On EVM, such instructions as `REVERT` and `INVALID`,
immediately terminate the contract execution and return the control to the callee. It is impossible to catch them within
the contract, and it can be only done on the callee side with checking the call status code.

```solidity
// callee
revert(0, 0)

// caller
let success = call(...)
if iszero(success) {
    // option 1: rethrow on the contract level
    returndatacopy(...)
    revert(...)

    // option 2: rethrow on the function level
    verbatim("throw") // only available in the Yul mode and upcoming zkEVM solc
}
```

zkEVM behaves exactly the same. The VM automatically unwinds the call stack up to the uppermost function frame of the
contract, leaving no possibility to catch and handle it on the way.

These types of exceptions are more efficient, as you can revert at any point of the execution without propagating the
control flow all the way up to the uppermost function frame.

## Function Level

This type of exceptions is more common to general-purpose languages like C++. That is why it was easy to support within
the LLVM framework, even though it is not supported by the smart contract languages we work with. That is also one of
the reasons why the two EH mechanisms are handled separately and barely interact in the high-level code.

In general-purpose languages a set of EH tools is usually available, e.g. `try` , `throw`, and `catch` keywords that
define which piece of code may throw and how the exception must be handled. However, these tools are not available in
Solidity and its EVM Yul dialect, so some extensions have been added in the zkEVM Yul dialect compiled by zksolc, but
there are limitations, some of which are dictated by the nature of smart contracts:

1. Every function beginning with `ZKSYNC_NEAR_CALL` is implicitly wrapped with `try`. If there is an exception handler
   defined, the following will happen:
   - A panic will be caught by the caller of such function.
   - The control will be transferred to EH function. There can be only one EH function and it must be named
     `ZKSYNC_CATCH_NEAR_CALL`. It is not very efficient, because all functions must have an LLVM IR `catch` block that
     will catch and propagate the exception and call the EH function.
   - When the EH function has finished executing, the caller of `ZKSYNC_NEAR_CALL` receives the control back.
2. Every operation is `throw`. Since any instruction can panic due to out-of-gas, all of them can throw. It is another
   thing reducing the potential for optimizations.
3. The `catch` block is represented by the `ZKSYNC_CATCH_NEAR_CALL` function in Yul. A panic in `ZKSYNC_NEAR_CALL` will
   make **their caller** catch the exception and call the EH function. After the EH function is executed, the control is
   returned to the caller of `ZKSYNC_NEAR_CALL`.

```solidity
// Follow the numbers for the order of execution. The call order is:
// caller -> ZKSYNC_NEAR_CALL_callee -> callee_even_deeper -> ZKSYNC_CATCH_NEAR_CALL -> caller

function ZKSYNC_NEAR_CALL_callee() -> value {    // 03
    value := callee_even_deeper()                // 04
}

function callee_even_deeper() -> value {         // 05
    verbatim("throw")                            // 06
}

// In every function an implicit 'catch' block in LLVM IR is created.
// This block will do the following:
//     1. Keep the return value ('zero') zero-initialized if one is expected
//     2. Call the EH function ('ZKSYNC_CATCH_NEAR_CALL')
//     3. Return the control flow to the next instruction ('value := 42')
function caller() -> value {                      // 01
    let zero := ZKSYNC_NEAR_CALL_callee()         // 02
    value := 42                                   // 09
}

// This handler could also be doing a revert.
// Reverts in EH functions work in the same way as without EH at all.
// They immediately terminate the execution and the control is given to the contract callee.
function ZKSYNC_CATCH_NEAR_CALL() {               // 07
    log0(...)                                     // 08
}
```

Having all the overhead above, the `catch` blocks are only generated if there is the EH function
`ZKSYNC_CATCH_NEAR_CALL` defined in the contract. Otherwise there is no need to catch panics and they will be propagated
to the callee contract automatically by the VM execution environment.
