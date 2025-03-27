# [ADD](https://www.evm.codes/#01?fork=shanghai)

### LLVM IR

```llvm
%addition_result = add i256 %value1, %value2
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/arithmetic.rs#L15)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#add-instruction)

### EraVM Assembly

```nasm
add     r1, r2, r1
```

## [MUL](https://www.evm.codes/#02?fork=shanghai)

### Differences from EVM

1. The carry is written to the 2nd output register

### LLVM IR

```llvm
%multiplication_result = mul i256 %value1, %value2
```

EraVM can output the carry of the multiplication operation. In this case, the result is a tuple of two values: the
multiplication result and the carry. The carry is written to the 2nd output register. The snippet below returns the
carry value.

```llvm
%value1_extended = zext i256 %value1 to i512
%value2_extended = zext i256 %value2 to i512
%result_extended = mul nuw i512 %value1_extended, %value2_extended
%result_shifted = lshr i512 %result_extended, 256
%result = trunc i512 %result_shifted to i256
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/arithmetic.rs#L53)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#mul-instruction)

### EraVM Assembly

```nasm
mul     r1, r2, r1, r2
```

## [SUB](https://www.evm.codes/#03?fork=shanghai)

### LLVM IR

```llvm
%subtraction_result = sub i256 %value1, %value2
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/arithmetic.rs#L34)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#sub-instruction)

### EraVM Assembly

```nasm
sub     r1, r2, r1
```

## [DIV](https://www.evm.codes/#04?fork=shanghai)

### Differences from EVM

1. The remainder is written to the 2nd output register

### LLVM IR

```llvm
define i256 @__div(i256 %arg1, i256 %arg2) #0 {
entry:
  %is_divider_zero = icmp eq i256 %arg2, 0
  br i1 %is_divider_zero, label %return, label %division

division:
  %div_res = udiv i256 %arg1, %arg2
  br label %return

return:
  %res = phi i256 [ 0, %entry ], [ %div_res, %division ]
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/arithmetic.rs#L73)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#udiv-instruction)

## [SDIV](https://www.evm.codes/#05?fork=shanghai)

### LLVM IR

```llvm
define i256 @__sdiv(i256 %arg1, i256 %arg2) #0 {
entry:
  %is_divider_zero = icmp eq i256 %arg2, 0
  br i1 %is_divider_zero, label %return, label %division_overflow

division_overflow:
  %is_divided_int_min = icmp eq i256 %arg1, -57896044618658097711785492504343953926634992332820282019728792003956564819968
  %is_minus_one = icmp eq i256 %arg2, -1
  %is_overflow = and i1 %is_divided_int_min, %is_minus_one
  br i1 %is_overflow, label %return, label %division

division:
  %div_res = sdiv i256 %arg1, %arg2
  br label %return

return:
  %res = phi i256 [ 0, %entry ], [ %arg1, %division_overflow ], [ %div_res, %division ]
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/arithmetic.rs#L162)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#sdiv-instruction)

## [MOD](https://www.evm.codes/#06?fork=shanghai)

### Differences from EVM

1. The remainder is written to the 2nd output register

### LLVM IR

```llvm
define i256 @__mod(i256 %arg1, i256 %arg2) #0 {
entry:
  %is_divider_zero = icmp eq i256 %arg2, 0
  br i1 %is_divider_zero, label %return, label %remainder

remainder:
  %rem_res = urem i256 %arg1, %arg2
  br label %return

return:
  %res = phi i256 [ 0, %entry ], [ %rem_res, %remainder ]
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/arithmetic.rs#L117)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#urem-instruction)

## [SMOD](https://www.evm.codes/#07?fork=shanghai)

### LLVM IR

```llvm
define i256 @__smod(i256 %arg1, i256 %arg2) #0 {
entry:
  %is_divider_zero = icmp eq i256 %arg2, 0
  br i1 %is_divider_zero, label %return, label %division_overflow

division_overflow:
  %is_divided_int_min = icmp eq i256 %arg1, -57896044618658097711785492504343953926634992332820282019728792003956564819968
  %is_minus_one = icmp eq i256 %arg2, -1
  %is_overflow = and i1 %is_divided_int_min, %is_minus_one
  br i1 %is_overflow, label %return, label %remainder

remainder:
  %rem_res = srem i256 %arg1, %arg2
  br label %return

return:
  %res = phi i256 [ 0, %entry ], [ 0, %division_overflow ], [ %rem_res, %remainder ]
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/arithmetic.rs#L236)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#srem-instruction)

## [ADDMOD](https://www.evm.codes/#08?fork=shanghai)

### LLVM IR

```llvm
define i256 @__addmod(i256 %arg1, i256 %arg2, i256 %modulo) #0 {
entry:
  %is_zero = icmp eq i256 %modulo, 0
  br i1 %is_zero, label %return, label %addmod

addmod:
  %arg1m = urem i256 %arg1, %modulo
  %arg2m = urem i256 %arg2, %modulo
  %res = call {i256, i1} @llvm.uadd.with.overflow.i256(i256 %arg1m, i256 %arg2m)
  %sum = extractvalue {i256, i1} %res, 0
  %obit = extractvalue {i256, i1} %res, 1
  %sum.mod = urem i256 %sum, %modulo
  br i1 %obit, label %overflow, label %return

overflow:
  %mod.inv = xor i256 %modulo, -1
  %sum1 = add i256 %sum, %mod.inv
  %sum.ovf = add i256 %sum1, 1
  br label %return

return:
  %value = phi i256 [0, %entry], [%sum.mod, %addmod], [%sum.ovf, %overflow]
  ret i256 %value
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/math.rs#L16)
is common for Yul and EVMLA representations.

## [MULMOD](https://www.evm.codes/#09?fork=shanghai)

### LLVM IR

```llvm
define i256 @__mulmod(i256 %arg1, i256 %arg2, i256 %modulo) #0 {
entry:
  %cccond = icmp eq i256 %modulo, 0
  br i1 %cccond, label %ccret, label %entrycont

ccret:
  ret i256 0

entrycont:
  %arg1m = urem i256 %arg1, %modulo
  %arg2m = urem i256 %arg2, %modulo
  %less_then_2_128 = icmp ult i256 %modulo, 340282366920938463463374607431768211456
  br i1 %less_then_2_128, label %fast, label %slow

fast:
  %prod = mul i256 %arg1m, %arg2m
  %prodm = urem i256 %prod, %modulo
  ret i256 %prodm

slow:
  %arg1e = zext i256 %arg1m to i512
  %arg2e = zext i256 %arg2m to i512
  %prode = mul i512 %arg1e, %arg2e
  %prodl = trunc i512 %prode to i256
  %prodeh = lshr i512 %prode, 256
  %prodh = trunc i512 %prodeh to i256
  %res = call i256 @__ulongrem(i256 %prodl, i256 %prodh, i256 %modulo)
  ret i256 %res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/math.rs#L43)
is common for Yul and EVMLA representations.

## [EXP](https://www.evm.codes/#0a?fork=shanghai)

### LLVM IR

```llvm
define i256 @__exp(i256 %value, i256 %exp) "noinline-oz" #0 {
entry:
  %exp_is_non_zero = icmp eq i256 %exp, 0
  br i1 %exp_is_non_zero, label %return, label %exponent_loop_body

return:
  %exp_res = phi i256 [ 1, %entry ], [ %exp_res.1, %exponent_loop_body ]
  ret i256 %exp_res

exponent_loop_body:
  %exp_res.2 = phi i256 [ %exp_res.1, %exponent_loop_body ], [ 1, %entry ]
  %exp_val = phi i256 [ %exp_val_halved, %exponent_loop_body ], [ %exp, %entry ]
  %val_squared.1 = phi i256 [ %val_squared, %exponent_loop_body ], [ %value, %entry ]
  %odd_test = and i256 %exp_val, 1
  %is_exp_odd = icmp eq i256 %odd_test, 0
  %exp_res.1.interm = select i1 %is_exp_odd, i256 1, i256 %val_squared.1
  %exp_res.1 = mul i256 %exp_res.1.interm, %exp_res.2
  %val_squared = mul i256 %val_squared.1, %val_squared.1
  %exp_val_halved = lshr i256 %exp_val, 1
  %exp_val_is_less_2 = icmp ult i256 %exp_val, 2
  br i1 %exp_val_is_less_2, label %return, label %exponent_loop_body
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/math.rs#L70)
is common for Yul and EVMLA representations.

## [SIGNEXTEND](https://www.evm.codes/#0b?fork=shanghai)

### LLVM IR

```llvm
define i256 @__signextend(i256 %numbyte, i256 %value) #0 {
entry:
  %is_overflow = icmp uge i256 %numbyte, 31
  br i1 %is_overflow, label %return, label %signextend

signextend:
  %numbit_byte = mul nuw nsw i256 %numbyte, 8
  %numbit = add nsw nuw i256 %numbit_byte, 7
  %numbit_inv = sub i256 256, %numbit
  %signmask = shl i256 1, %numbit
  %valmask = lshr i256 -1, %numbit_inv
  %ext1 = shl i256 -1, %numbit
  %signv = and i256 %signmask, %value
  %sign = icmp ne i256 %signv, 0
  %valclean = and i256 %value, %valmask
  %sext = select i1 %sign, i256 %ext1, i256 0
  %result = or i256 %sext, %valclean
  br label %return

return:
  %signext_res = phi i256 [%value, %entry], [%result, %signextend]
  ret i256 %signext_res
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/math.rs#L93)
is common for Yul and EVMLA representations.
