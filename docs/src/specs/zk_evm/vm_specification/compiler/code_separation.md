# Deploy and Runtime Code Separation

On both EVM and EraVM the code is separated into two parts: deploy code and runtime code. The deploy code is executed
only once, when the contract is deployed. The runtime code is executed every time the contract is called. However, on
EraVM the deploy code and runtime code are deployed together, and they are not split into two separate chunks of
bytecode.

The constructor is added to the contract as a regular public function which is called by System Contracts during
deployment.

Just like on EVM, the deploy code on EraVM is represented by a single constructor, named differently in different
languages:

| Language | Name              |
| -------- | ----------------- |
| Solidity | `constructor`     |
| Yul      | `object "<name>"` |
| Vyper    | `__init__`        |

The constructor is merged with the runtime code by the LLVM IR generator of our compiler, and a minimal contract on
EraVM looks like on the examples below.

### LLVM IR

In the example below, contract `@__entry` arguments `%0`-`%11` correspond to registers `r1`-`r12` on EraVM.

```llvm
; Function Attrs: nofree noreturn null_pointer_is_valid
define i256 @__entry(ptr addrspace(3) nocapture readnone %0, i256 %1, i256 %2, i256 %3, i256 %4, i256 %5, i256 %6, i256 %7, i256 %8, i256 %9, i256 %10, i256 %11) local_unnamed_addr #1 personality ptr @__personality {
entry:
  %is_deploy_code_call_flag_truncated = and i256 %1, 1                                                          ; check if the call is a deploy code call
  %is_deploy_code_call_flag.not = icmp eq i256 %is_deploy_code_call_flag_truncated, 0                           ; invert the flag
  br i1 %is_deploy_code_call_flag.not, label %runtime_code_call_block, label %deploy_code_call_block            ; branch to the deploy code block if the flag is set

deploy_code_call_block:                           ; preds = %entry
  store i256 32, ptr addrspace(2) inttoptr (i256 256 to ptr addrspace(2)), align 256                            ; store the offset of the array of immutables
  store i256 0, ptr addrspace(2) inttoptr (i256 288 to ptr addrspace(2)), align 32                              ; store the length of the array of immutables
  tail call void @llvm.syncvm.return(i256 53919893334301279589334030174039261352344891250716429051063678533632) ; return the array of immutables using EraVM return ABI data encoding
  unreachable

runtime_code_call_block:                          ; preds = %entry
  store i256 42, ptr addrspace(1) null, align 4294967296                                                        ; store a value to return
  tail call void @llvm.syncvm.return(i256 2535301200456458802993406410752)                                      ; return the value using EraVM return ABI data encoding
  unreachable
}
```

### EraVM Assembly

```nasm
        .text
        .file   "default.yul"
        .globl  __entry
__entry:
.func_begin0:
        and!    1, r2, r1                           ; check if the call is a deploy code call
        jump.ne @.BB0_1                             ; branch to the deploy code block if the flag is set
        add     42, r0, r1                          ; move the value to return into r1
        st.1    0, r1                               ; store the value to return
        add     @CPI0_1[0], r0, r1                  ; move the return ABI data into r1
        ret.ok.to_label r1, @DEFAULT_FAR_RETURN     ; return the value
.BB0_1:
        add     32, r0, r1                          ; move the offset of the array of immutables into r1
        st.2    256, r1                             ; store the offset of the array of immutables
        st.2    288, r0                             ; store the length of the array of immutables
        add     @CPI0_0[0], r0, r1                  ; move the return ABI data into r1
        ret.ok.to_label r1, @DEFAULT_FAR_RETURN     ; return the array of immutables
.func_end0:

        .note.GNU-stack
        .rodata
CPI0_0:
        .cell 53919893334301279589334030174039261352344891250716429051063678533632
CPI0_1:
        .cell 2535301200456458802993406410752
```
