<!-- TODO review differences -->

# Elliptic curve precompiles

Precompiled contracts for elliptic curve operations are required in order to perform zkSNARK verification.

The operations that you need to be able to perform are elliptic curve point addition, elliptic curve point scalar multiplication, and elliptic curve pairing.

This document explains the precompiles responsible for elliptic curve point addition and scalar multiplication and the design decisions. You can read the specification [here](https://eips.ethereum.org/EIPS/eip-196).

## Introduction

On top of having a set of opcodes to choose from, the EVM also offers a set of more advanced functionalities through precompiled contracts. These are a special kind of contracts that are bundled with the EVM at fixed addresses and can be called with a determined gas cost. The addresses start from 1, and increment for each contract. New hard forks may introduce new precompiled contracts. They are called from the opcodes like regular contracts, with instructions like CALL. The gas cost mentioned here is purely the cost of the contract and does not consider the cost of the call itself nor the instructions to put the parameters in memory.

For Go-Ethereum, the code being run is written in Go, and the gas costs are defined in each precompile spec.

In the case of ZKsync Era, ecAdd and ecMul precompiles are written as a smart contract for two reasons:

- zkEVM needs to be able to prove their execution (and at the moment it cannot do that if the code being run is executed outside the VM)
- Writing custom circuits for Elliptic curve operations is hard, and time-consuming, and after all such code is harder to maintain and audit.

## Field Arithmetic

The BN254 (also known as alt-BN128) is an elliptic curve defined by the equation $y^2 = x^3 + 3$ over the finite field $\mathbb{F}_p$, being $p = 21888242871839275222246405745257275088696311157297823662689037894645226208583. The modulus is less than 256 bits, which is why every element in the field is represented as a `uint256`.

The arithmetic is carried out with the field elements encoded in the Montgomery form. This is done not only because operating in the Montgomery form speeds up the computation but also because the native modular multiplication, which is carried out by Yul's `mulmod` opcode, is very inefficient.

Instructions set on ZKsync and EVM are different, so the performance of the same Yul/Solidity code can be efficient on EVM, but not on zkEVM and opposite.

One such very inefficient command is `mulmod`. On EVM there is a native opcode that makes modulo multiplication and it costs only 8 gas, which compared to the other opcodes costs is only 2-3 times more expensive. On zkEVM we don’t have native `mulmod` opcode, instead, the compiler does full-with multiplication (e.g. it multiplies two `uint256`s and gets as a result an `uint512`). Then the compiler performs long division for reduction (but only the remainder is kept), in the generic form it is an expensive operation and costs many opcode executions, which can’t be compared to the cost of one opcode execution. The worst thing is that `mulmod` is used a lot for the modulo inversion, so optimizing this one opcode gives a huge benefit to the precompiles.

### Multiplication

As said before, multiplication was carried out by implementing the Montgomery reduction, which works with general moduli and provides a significant speedup compared to the naïve approach.

The squaring operation is obtained by multiplying a number by itself. However, this operation can have an additional speedup by implementing the SOS Montgomery squaring.

### Inversion

Inversion was performed using the extended binary Euclidean algorithm (also known as extended binary greatest common divisor). This algorithm is a modification of Algorithm 3 `MontInvbEEA` from [Montgomery inversion](https://cetinkayakoc.net/docs/j82.pdf).

### Exponentiation

The exponentiation was carried out using the square and multiply algorithm, which is a standard technique for this operation.

## Montgomery Form

Let’s take a number `R`, such that `gcd(N, R) == 1` and `R` is a number by which we can efficiently divide and take module over it (for example power of two or better machine word, aka 2^256). Then transform every number to the form of `x * R mod N` / `y * R mod N` and then we get efficient modulo addition and multiplication. The only thing is that before working with numbers we need to transform them to the form from `x mod N` to the `x * R mod N` and after performing operations transform the form back.

For the latter, we will assume that `N` is the module that we use in computations, and `R` is $2^{256}$, since we can efficiently divide and take module over this number and it practically satisfies the property of `gcd(N, R) == 1`.

### Montgomery Reduction Algorithm (REDC)

> Reference: <https://en.wikipedia.org/wiki/Montgomery_modular_multiplication#The_REDC_algorithm>

```solidity
/// @notice Implementation of the Montgomery reduction algorithm (a.k.a. REDC).
/// @dev See <https://en.wikipedia.org/wiki/Montgomery_modular_multiplication#The_REDC_algorithm>
/// @param lowestHalfOfT The lowest half of the value T.
/// @param higherHalfOfT The higher half of the value T.
/// @return S The result of the Montgomery reduction.
function REDC(lowestHalfOfT, higherHalfOfT) -> S {
    let q := mul(lowestHalfOfT, N_PRIME())
    let aHi := add(higherHalfOfT, getHighestHalfOfMultiplication(q, P()))
    let aLo, overflowed := overflowingAdd(lowestHalfOfT, mul(q, P()))
    if overflowed {
        aHi := add(aHi, 1)
    }
    S := aHi
    if iszero(lt(aHi, P())) {
        S := sub(aHi, P())
    }
}

```

By choosing $R = 2^{256}$ we avoided 2 modulo operations and one division from the original algorithm. This is because in Yul, native numbers are uint256 and the modulo operation is native, but for the division, as we work with a 512-bit number split into two parts (high and low part) dividing by $R$ means shifting 256 bits to the right or what is the same, discarding the low part.

### Montgomery Addition/Subtraction

Addition and subtraction in Montgomery form are the same as ordinary modular addition and subtraction because of the distributive law

$$
\begin{align*}
aR+bR=(a+b)R,\\
aR-bR=(a-b)R.
\end{align*}
$$

```solidity
/// @notice Computes the Montgomery addition.
/// @dev See <https://en.wikipedia.org/wiki/Montgomery_modular_multiplication#The_The_REDC_algorithm> for further details on the Montgomery multiplication.
/// @param augend The augend in Montgomery form.
/// @param addend The addend in Montgomery form.
/// @return ret The result of the Montgomery addition.
function montgomeryAdd(augend, addend) -> ret {
    ret := add(augend, addend)
    if iszero(lt(ret, P())) {
        ret := sub(ret, P())
    }
}

/// @notice Computes the Montgomery subtraction.
/// @dev See <https://en.wikipedia.org/wiki/Montgomery_modular_multiplication#The_The_REDC_algorithm> for further details on the Montgomery multiplication.
/// @param minuend The minuend in Montgomery form.
/// @param subtrahend The subtrahend in Montgomery form.
/// @return ret The result of the Montgomery subtraction.
function montgomerySub(minuend, subtrahend) -> ret {
    ret := montgomeryAdd(minuend, sub(P(), subtrahend))
}

```

We do not use `addmod`. That's because in most cases the sum does not exceed the modulus.

### Montgomery Multiplication

The product of $aR \mod N$ and $bR \mod N$ is $REDC((aR \mod N)(bR \mod N))$.

```solidity
/// @notice Computes the Montgomery multiplication using the Montgomery reduction algorithm (REDC).
/// @dev See <https://en.wikipedia.org/wiki/Montgomery_modular_multiplication#The_The_REDC_algorithm> for further details on the Montgomery multiplication.
/// @param multiplicand The multiplicand in Montgomery form.
/// @param multiplier The multiplier in Montgomery form.
/// @return ret The result of the Montgomery multiplication.
function montgomeryMul(multiplicand, multiplier) -> ret {
    let hi := getHighestHalfOfMultiplication(multiplicand, multiplier)
    let lo := mul(multiplicand, multiplier)
    ret := REDC(lo, hi)
}

```

### Montgomery Inversion

```solidity
/// @notice Computes the Montgomery modular inverse skipping the Montgomery reduction step.
/// @dev The Montgomery reduction step is skipped because a modification in the binary extended Euclidean algorithm is used to compute the modular inverse.
/// @dev See the function `binaryExtendedEuclideanAlgorithm` for further details.
/// @param a The field element in Montgomery form to compute the modular inverse of.
/// @return invmod The result of the Montgomery modular inverse (in Montgomery form).
function montgomeryModularInverse(a) -> invmod {
    invmod := binaryExtendedEuclideanAlgorithm(a)
}
```

As said before, we use a modified version of the bEE algorithm that lets us “skip” the Montgomery reduction step.

The regular algorithm would be $REDC((aR \mod N)^{−1}(R^3 \mod N))$ which involves a regular inversion plus a multiplication by a value that can be precomputed.

## ECADD

Precompile for computing elliptic curve point addition. The points are represented in affine form, given by a pair of coordinates $(x,y)$.

Affine coordinates are the conventional way of expressing elliptic curve points, which use 2 coordinates. The math is concise and easy to follow.

For a pair of constants $a$ and $b$, an elliptic curve is defined by the set of all points $(x,y)$ that satisfy the equation $y^2=x^3+ax+b$, plus a special “point at infinity” named $O$.

### Point Doubling

To compute $2P$ (or $P+P$), there are three cases:

- If $P = O$, then $2P = O$.
- Else $P = (x, y)$

  - If $y = 0$, then $2P = O$.
  - Else $y≠0$, then

    $$
    \begin{gather*} \lambda = \frac{3x_{p}^{2} + a}{2y_{p}} \\ x_{r} = \lambda^{2} - 2x_{p} \\ y_{r} = \lambda(x_{p} - x_{r}) - y_{p}\end{gather*}
    $$

The complicated case involves approximately 6 multiplications, 4 additions/subtractions, and 1 division. There could also be 4 multiplications, 6 additions/subtractions, and 1 division, and if you want you could trade a multiplication with 2 more additions.

### Point Addition

To compute $P + Q$ where $P \neq Q$, there are four cases:

- If $P = 0$ and $Q \neq 0$, then $P + Q = Q$.
- If $Q = 0$ and $P \neq 0$, then $P + Q = P$.
- Else $P = (x_{p},\ y_{p})$ and$Q = (x_{q},\ y_{q})$

  - If $x_{p} = x_{q}$ (and necessarily $y_{p} \neq y_{q}$), then $P + Q = O$.
  - Else $x_{p} \neq x_{q}$, then

    $$
    \begin{gather*} \lambda = \frac{y_{2} - y_{1}}{x_{2} - x_{1}} \\ x_{r} = \lambda^{2} - x_{p} - x_{q} \\ y_{r} = \lambda(x_{p} - x_{r}) - y_{p}\end{gather*}
    $$

    and $P + Q = R = (x_{r},\ y_{r})$.

The complicated case involves approximately 2 multiplications, 6 additions/subtractions, and 1 division.

## ECMUL

Precompile for computing elliptic curve point scalar multiplication. The points are represented in homogeneous projective coordinates, given by the coordinates $(x , y , z)$. Transformation into affine coordinates can be done by applying the following transformation:
$(x,y) = (X.Z^{-1} , Y.Z^{-1} )$ if the point is not the point at infinity.

The key idea of projective coordinates is that instead of performing every division immediately, we defer the divisions by multiplying them into a denominator. The denominator is represented by a new coordinate. Only at the very end, do we perform a single division to convert from projective coordinates back to affine coordinates.

In affine form, each elliptic curve point has 2 coordinates, like $(x,y)$. In the new projective form, each point will have 3 coordinates, like $(X,Y,Z)$, with the restriction that $Z$ is never zero. The forward mapping is given by $(x,y)→(xz,yz,z)$, for any non-zero $z$ (usually chosen to be 1 for convenience). The reverse mapping is given by $(X,Y,Z)→(X/Z,Y/Z)$, as long as $Z$ is non-zero.

### Point Doubling

The affine form case $y=0$ corresponds to the projective form case $Y/Z=0$. This is equivalent to $Y=0$, since $Z≠0$.

For the interesting case where $P=(X,Y,Z)$ and $Y≠0$, let’s convert the affine arithmetic to projective arithmetic.

After expanding and simplifying the equations ([demonstration here](https://www.nayuki.io/page/elliptic-curve-point-addition-in-projective-coordinates)), the following substitutions come out

$$
\begin{align*} T &= 3X^{2} + aZ^{2},\\ U &= 2YZ,\\ V &= 2UXY,\\ W &= T^{2} - 2V \end{align*}
$$

Using them, we can write

$$
\begin{align*} X_{r}  &= UW \\ Y_{r} &= T(V−W)−2(UY)^{2} \\ Z_{r} &= U^{3} \end{align*}
$$

As we can see, the complicated case involves approximately 18 multiplications, 4 additions/subtractions, and 0 divisions.

### Point Addition

The affine form case $x_{p} = x_{q}$ corresponds to the projective form case $X_{p}/Z_{p} = X_{q}/Z_{q}$. This is equivalent to $X_{p}Z_{q} = X_{q}Z_{p}$, via cross-multiplication.

For the interesting case where $P = (X_{p},\ Y_{p},\ Z_{p})$ , $Q = (X_{q},\ Y_{q},\ Z_{q})$, and $X_{p}Z_{q} ≠ X_{q}Z_{p}$, let’s convert the affine arithmetic to projective arithmetic.

After expanding and simplifying the equations ([demonstration here](https://www.nayuki.io/page/elliptic-curve-point-addition-in-projective-coordinates)), the following substitutions come out

$$
\begin{align*}
T_{0} &= Y_{p}Z_{q}\\
T_{1} &= Y_{q}Z_{p}\\
T &= T_{0} - T_{1}\\
U_{0} &= X_{p}Z_{q}\\
U_{1} &= X_{q}Z_{p}\\
U &= U_{0} - U_{1}\\
U_{2} &= U^{2}\\
V &= Z_{p}Z_{q}\\
W &= T^{2}V−U_{2}(U_{0}+U_{1}) \\
\end{align*}
$$

Using them, we can write

$$
\begin{align*} X_{r}  &= UW \\ Y_{r} &= T(U_{0}U_{2}−W)−T_{0}U^{3} \\ Z_{r} &= U^{3}V \end{align*}
$$

As we can see, the complicated case involves approximately 15 multiplications, 6 additions/subtractions, and 0 divisions.
