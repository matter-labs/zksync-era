# [AND](https://www.evm.codes/#16?fork=shanghai)

### LLVM IR

```llvm
%and_result = and i256 %value1, %value2
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/bitwise.rs#L47)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#and-instruction)

### EraVM Assembly

```nasm
ptr.add stack[@ptr_calldata], r0, r1
ptr.add.s       36, r1, r2
ld      r2, r2
ptr.add.s       4, r1, r1
ld      r1, r1
and     r1, r2, r1
st.1    128, r1
```

## [OR](https://www.evm.codes/#17?fork=shanghai)

### LLVM IR

```llvm
%or_result = or i256 %value1, %value2
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/bitwise.rs#L13)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#or-instruction)

### EraVM Assembly

```nasm
ptr.add stack[@ptr_calldata], r0, r1
ptr.add.s       36, r1, r2
ld      r2, r2
ptr.add.s       4, r1, r1
ld      r1, r1
or      r1, r2, r1
st.1    128, r1
```

## [XOR](https://www.evm.codes/#18?fork=shanghai)

### LLVM IR

```llvm
%xor_result = or i256 %value1, %value2
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/bitwise.rs#L30)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#xor-instruction)

### EraVM Assembly

```nasm
ptr.add stack[@ptr_calldata], r0, r1
ptr.add.s       36, r1, r2
ld      r2, r2
ptr.add.s       4, r1, r1
ld      r1, r1
xor     r1, r2, r1
st.1    128, r1
```

## [NOT](https://www.evm.codes/#19?fork=shanghai)

### LLVM IR

```llvm
%xor_result = xor i256 %value, -1
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/bitwise.rs#L30)
is common for Yul and EVMLA representations.

### EraVM Assembly

```nasm
ptr.add stack[@ptr_calldata], r1, r1
ld      r1, r1
sub.s   1, r0, r2
xor     r1, r2, r1
st.1    128, r1
```

## [BYTE](https://www.evm.codes/#1a?fork=shanghai)

### LLVM IR

```llvm
define i256 @__byte(i256 %index, i256 %value) #0 {
entry:
  %is_overflow = icmp ugt i256 %index, 31
  br i1 %is_overflow, label %return, label %extract_byte

extract_byte:
  %bits_offset = shl i256 %index, 3
  %value_shifted_left = shl i256 %value, %bits_offset
  %value_shifted_right = lshr i256 %value_shifted_left, 248
  br label %return

return:
  %res = phi i256 [ 0, %entry ], [ %value_shifted_right, %extract_byte ]
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/bitwise.rs#L229)
is common for Yul and EVMLA representations.

## [SHL](https://www.evm.codes/#1b?fork=shanghai)

### LLVM IR

```llvm
define i256 @__shl(i256 %shift, i256 %value) #0 {
entry:
  %is_overflow = icmp ugt i256 %shift, 255
  br i1 %is_overflow, label %return, label %shift_value

shift_value:
  %shift_res = shl i256 %value, %shift
  br label %return

return:
  %res = phi i256 [ 0, %entry ], [ %shift_res, %shift_value ]
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/bitwise.rs#L67)
is common for Yul and EVMLA representations.

## [SHR](https://www.evm.codes/#1c?fork=shanghai)

### LLVM IR

```llvm
define i256 @__shr(i256 %shift, i256 %value) #0 {
entry:
  %is_overflow = icmp ugt i256 %shift, 255
  br i1 %is_overflow, label %return, label %shift_value

shift_value:
  %shift_res = lshr i256 %value, %shift
  br label %return

return:
  %res = phi i256 [ 0, %entry ], [ %shift_res, %shift_value ]
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/bitwise.rs#L111)
is common for Yul and EVMLA representations.

## [SAR](https://www.evm.codes/#1d?fork=shanghai)

### LLVM IR

```llvm
define i256 @__sar(i256 %shift, i256 %value) #0 {
entry:
  %is_overflow = icmp ugt i256 %shift, 255
  br i1 %is_overflow, label %arith_overflow, label %shift_value

arith_overflow:
  %is_val_positive = icmp sge i256 %value, 0
  %res_overflow = select i1 %is_val_positive, i256 0, i256 -1
  br label %return

shift_value:
  %shift_res = ashr i256 %value, %shift
  br label %return

return:
  %res = phi i256 [ %res_overflow, %arith_overflow ], [ %shift_res, %shift_value ]
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/bitwise.rs#L157)
is common for Yul and EVMLA representations.
