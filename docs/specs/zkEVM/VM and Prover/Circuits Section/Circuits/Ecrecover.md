# Ecrecover

## Ecrecover PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/fsm_input_output/circuit_inputs/main_vm.rs#L9)

```rust
pub struct PrecompileFunctionInputData<F: SmallField> {
    pub initial_log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub initial_memory_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/base_structures/precompile_input_outputs/mod.rs#L42)

```rust
pub struct PrecompileFunctionOutputData<F: SmallField> {
    pub final_memory_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/keccak256_round_function/input.rs#L59)

```rust
pub struct EcrecoverCircuitFSMInputOutput<F: SmallField> {
    pub log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub memory_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

## Main circuit logic

This circuit implements the ecrecover precompile described in the Ethereum yellow paper: https://ethereum.github.io/yellowpaper/paper.pdf

The purpose of ecrecover is to recover the signer’s public key from digital signature.

A special note about this circuit is that there are hardcoded ‘valid’ field element values provided to the circuit. This is to prevent the circuit from not satisfying in case the user-provided inputs are incorrect and, when the circuit detects this, the bad values are swapped out for the hardcoded ones. In this event, exceptions are logged and pushed into a vector which are returned to the caller, informing them that the provided inputs were incorrect and the result should be discarded.

Most of the relevant circuit logic resides in the `ecrecover_precompile_inner_routine` function. Let’s take the circuit step by step.

1. The circuit starts off by declaring a set of constants which are useful to have throughout the circuit. These include the B parameter of the secp256k1 curve, the constant -1 in the curve’s base field, and the base field and scalar field modulus. We also create the vector that should capture any exceptions.

```rust
let curve_b = Secp256Affine::b_coeff();

let mut minus_one = Secp256Fq::one();
minus_one.negate();

let mut curve_b_nn =
    Secp256BaseNNField::<F>::allocated_constant(cs, curve_b, &base_field_params);
let mut minus_one_nn =
    Secp256BaseNNField::<F>::allocated_constant(cs, minus_one, &base_field_params);

let secp_n_u256 = U256([
    scalar_field_params.modulus_u1024.as_ref().as_words()[0],
    scalar_field_params.modulus_u1024.as_ref().as_words()[1],
    scalar_field_params.modulus_u1024.as_ref().as_words()[2],
    scalar_field_params.modulus_u1024.as_ref().as_words()[3],
]);
let secp_n_u256 = UInt256::allocated_constant(cs, secp_n_u256);

let secp_p_u256 = U256([
    base_field_params.modulus_u1024.as_ref().as_words()[0],
    base_field_params.modulus_u1024.as_ref().as_words()[1],
    base_field_params.modulus_u1024.as_ref().as_words()[2],
    base_field_params.modulus_u1024.as_ref().as_words()[3],
]);
let secp_p_u256 = UInt256::allocated_constant(cs, secp_p_u256);

let mut exception_flags = ArrayVec::<_, EXCEPTION_FLAGS_ARR_LEN>::new();
```

1. Next, the circuit checks whether or not the given `x` input (which is the x-coordinate of the signature) falls within the scalar field of the curve. Since, in ecrecover, `x = r + kn`, almost any `r` will encode a unique x-coordinate, except for when `r > scalar_field_modulus`. If this is the case, `x = r + n`, otherwise, `x = r`. `x` is recovered here from `r`.

```rust
let [y_is_odd, x_overflow, ..] =
    Num::<F>::from_variable(recid.get_variable()).spread_into_bits::<_, 8>(cs);

let (r_plus_n, of) = r.overflowing_add(cs, &secp_n_u256);
let mut x_as_u256 = UInt256::conditionally_select(cs, x_overflow, &r_plus_n, &r);
let error = Boolean::multi_and(cs, &[x_overflow, of]);
exception_flags.push(error);

// we handle x separately as it is the only element of base field of a curve (not a scalar field element!)
// check that x < q - order of base point on Secp256 curve
// if it is not actually the case - mask x to be zero
let (_res, is_in_range) = x_as_u256.overflowing_sub(cs, &secp_p_u256);
x_as_u256 = x_as_u256.mask(cs, is_in_range);
let x_is_not_in_range = is_in_range.negated(cs);
exception_flags.push(x_is_not_in_range);
```

1. Then, all field elements are interpreted as such within the circuit. As they are passed in, they are simply byte arrays which are interpreted initially as `UInt256` numbers. These get converted to field elements by using the conversion functions defined near the top of the file. Additionally, checks are done to make sure none of the passed in field elements are zero.

```rust
let mut x_fe = convert_uint256_to_field_element(cs, &x_as_u256, &base_field_params);

let (mut r_fe, r_is_zero) =
    convert_uint256_to_field_element_masked(cs, &r, &scalar_field_params);
exception_flags.push(r_is_zero);
let (mut s_fe, s_is_zero) =
    convert_uint256_to_field_element_masked(cs, &s, &scalar_field_params);
exception_flags.push(s_is_zero);

// NB: although it is not strictly an exception we also assume that hash is never zero as field element
let (mut message_hash_fe, message_hash_is_zero) =
    convert_uint256_to_field_element_masked(cs, &message_hash, &scalar_field_params);
exception_flags.push(message_hash_is_zero);
```

1. Now we are going to compute `t` and check whether or not it is quadratic residue in the base field. To start, we take `x` which we calculated before, and calculate `t` by doing `x^3 + b`, where `b` is the B parameter of the secp256k1 curve. We check to make sure that `t` is not zero.

```rust
let mut t = x_fe.square(cs);    // x^2
t = t.mul(cs, &mut x_fe);       // x^3
t = t.add(cs, &mut curve_b_nn); // x^3 + b

let t_is_zero = t.is_zero(cs);
exception_flags.push(t_is_zero);
```

1. The Legendre symbol for `t` is computed to do a quadratic residue check. We need to compute `t^b` which corresponds to `t^{2^255} / ( t^{2^31} * t^{2^8} * t^{2^7} * t^{2^6} * t^{2^5} * t^{2^3} * t)`. First, an array of powers of `t` is created (up to `t^255`). Then, we multiply together all the elements in the denominator of the equation, which are `t^{2^31} * t^{2^8} * t^{2^7} * t^{2^6} * t^{2^5} * t^{2^3} * t`. Lastly, the division is performed and we end up with `t^b`.

```rust
let t_is_zero = t.is_zero(cs); // We first do a zero check
exception_flags.push(t_is_zero);

// if t is zero then just mask
let t = Selectable::conditionally_select(cs, t_is_zero, &valid_t_in_external_field, &t);

// array of powers of t of the form t^{2^i} starting from i = 0 to 255
let mut t_powers = Vec::with_capacity(X_POWERS_ARR_LEN);
t_powers.push(t);

for _ in 1..X_POWERS_ARR_LEN {
    let prev = t_powers.last_mut().unwrap();
    let next = prev.square(cs);
    t_powers.push(next);
}

let mut acc = t_powers[0].clone();
for idx in [3, 5, 6, 7, 8, 31].into_iter() {
    let other = &mut t_powers[idx];
    acc = acc.mul(cs, other);
}
let mut legendre_symbol = t_powers[255].div_unchecked(cs, &mut acc);
```

1. Before we proceed to the quadratic residue check, we take advantage of the powers we just calculated to compute the square root of `t`, in order to determine whether the y-coordinate of the signature we’ve passed is positive or negative.

```rust
let mut acc_2 = t_powers[2].clone();
for idx in [4, 5, 6, 7, 30].into_iter() {
    let other = &mut t_powers[idx];
    acc_2 = acc_2.mul(cs, other);
}

let mut may_be_recovered_y = t_powers[254].div_unchecked(cs, &mut acc_2);
may_be_recovered_y.normalize(cs);
let mut may_be_recovered_y_negated = may_be_recovered_y.negated(cs);
may_be_recovered_y_negated.normalize(cs);

let [lowest_bit, ..] =
    Num::<F>::from_variable(may_be_recovered_y.limbs[0]).spread_into_bits::<_, 16>(cs);

// if lowest bit != parity bit, then we need conditionally select
let should_swap = lowest_bit.xor(cs, y_is_odd);
let may_be_recovered_y = Selectable::conditionally_select(
    cs,
    should_swap,
    &may_be_recovered_y_negated,
    &may_be_recovered_y,
);
```

1. Then, proceed with the quadratic residue check. In case `t` is nonresidue, we swap out our inputs for the hardcoded ‘valid’ inputs.

```rust
let t_is_nonresidue =
    Secp256BaseNNField::<F>::equals(cs, &mut legendre_symbol, &mut minus_one_nn);
exception_flags.push(t_is_nonresidue);
// unfortunately, if t is found to be a quadratic nonresidue, we can't simply let x to be zero,
// because then t_new = 7 is again a quadratic nonresidue. So, in this case we let x to be 9, then
// t = 16 is a quadratic residue
let x =
    Selectable::conditionally_select(cs, t_is_nonresidue, &valid_x_in_external_field, &x_fe);
let y = Selectable::conditionally_select(
    cs,
    t_is_nonresidue,
    &valid_y_in_external_field,
    &may_be_recovered_y,
);
```

1. The next step is computing the public key. We compute the public key `Q` by calculating `Q = (s * X - hash * G) / r`. We can simplify this in-circuit by calculating `s / r` and `hash / r` separately, and then doing an MSM to get the combined output. First, we pre-compute these divided field elements, and then compute the point like so:

```rust
let mut r_fe_inversed = r_fe.inverse_unchecked(cs);
let mut s_by_r_inv = s_fe.mul(cs, &mut r_fe_inversed);
let mut message_hash_by_r_inv = message_hash_fe.mul(cs, &mut r_fe_inversed);

s_by_r_inv.normalize(cs);
message_hash_by_r_inv.normalize(cs);

let mut gen_negated = Secp256Affine::one();
gen_negated.negate();
let (gen_negated_x, gen_negated_y) = gen_negated.into_xy_unchecked();
let gen_negated_x =
    Secp256BaseNNField::allocated_constant(cs, gen_negated_x, base_field_params);
let gen_negated_y =
    Secp256BaseNNField::allocated_constant(cs, gen_negated_y, base_field_params);

let s_by_r_inv_normalized_lsb_bits: Vec<_> = s_by_r_inv
    .limbs
    .iter()
    .map(|el| Num::<F>::from_variable(*el).spread_into_bits::<_, 16>(cs))
    .flatten()
    .collect();
let message_hash_by_r_inv_lsb_bits: Vec<_> = message_hash_by_r_inv
    .limbs
    .iter()
    .map(|el| Num::<F>::from_variable(*el).spread_into_bits::<_, 16>(cs))
    .flatten()
    .collect();

let mut recovered_point = (x, y);
let mut generator_point = (gen_negated_x, gen_negated_y);
// now we do multiexponentiation
let mut q_acc =
    SWProjectivePoint::<F, Secp256Affine, Secp256BaseNNField<F>>::zero(cs, base_field_params);

// we should start from MSB, double the accumulator, then conditionally add
for (cycle, (x_bit, hash_bit)) in s_by_r_inv_normalized_lsb_bits
    .into_iter()
    .rev()
    .zip(message_hash_by_r_inv_lsb_bits.into_iter().rev())
    .enumerate()
{
    if cycle != 0 {
        q_acc = q_acc.double(cs);
    }
    let q_plus_x = q_acc.add_mixed(cs, &mut recovered_point);
    let mut q_0: SWProjectivePoint<F, Secp256Affine, NonNativeFieldOverU16<F, Secp256Fq, 17>> =
        Selectable::conditionally_select(cs, x_bit, &q_plus_x, &q_acc);

    let q_plux_gen = q_0.add_mixed(cs, &mut generator_point);
    let q_1 = Selectable::conditionally_select(cs, hash_bit, &q_plux_gen, &q_0);

    q_acc = q_1;
}

let ((mut q_x, mut q_y), is_infinity) =
    q_acc.convert_to_affine_or_default(cs, Secp256Affine::one());
exception_flags.push(is_infinity);
let any_exception = Boolean::multi_or(cs, &exception_flags[..]);

q_x.normalize(cs);
q_y.normalize(cs);
```

1. Now that we have our public key recovered, the last thing we will need to do is take the keccak hash of the public key and then take the first 20 bytes to recover the address.

```rust
let zero_u8 = UInt8::zero(cs);

let mut bytes_to_hash = [zero_u8; 64];
let it = q_x.limbs[..16]
    .iter()
    .rev()
    .chain(q_y.limbs[..16].iter().rev());

for (dst, src) in bytes_to_hash.array_chunks_mut::<2>().zip(it) {
    let limb = unsafe { UInt16::from_variable_unchecked(*src) };
    *dst = limb.to_be_bytes(cs);
}

let mut digest_bytes = keccak256(cs, &bytes_to_hash);
// digest is 32 bytes, but we need only 20 to recover address
digest_bytes[0..12].copy_from_slice(&[zero_u8; 12]); // empty out top bytes
digest_bytes.reverse();
```

1. At this point, we are basically done! What’s left now is to ensure we send a masked value in case of any exception, and then we can output the resulting address and any exceptions which occurred for the caller to handle. This wraps up the ecrecover circuit!

```rust
let written_value_unmasked = UInt256::from_le_bytes(cs, digest_bytes);

let written_value = written_value_unmasked.mask_negated(cs, any_exception);
let all_ok = any_exception.negated(cs);

(all_ok, written_value) // Return any exceptions and the resulting address value
```