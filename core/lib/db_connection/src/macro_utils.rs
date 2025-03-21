//! Miscellaneous helper macros.

/// Writes to a [`String`]. This is equivalent to `write!`, but without the need to `unwrap()` the result.
#[macro_export]
macro_rules! write_str {
    ($buffer:expr, $($args:tt)+) => {{
        use std::fmt::Write as _;
        let __buffer: &mut std::string::String = $buffer;
        std::write!(__buffer, $($args)+).unwrap(); // Writing to a string cannot result in an error
    }};
}

/// Writing a line to a [`String`]. This is equivalent to `writeln!`, but without the need
/// to `unwrap()` the result.
#[macro_export]
macro_rules! writeln_str {
    ($buffer:expr, $($args:tt)+) => {{
        use std::fmt::Write as _;
        let __buffer: &mut std::string::String = $buffer;
        std::writeln!(__buffer, $($args)+).unwrap(); // Writing to a string cannot result in an error
    }};
}

/// Interpolates the provided DB query with string literals and variable substitutions.
///
/// Each part of the query can be either a string literal or `_`. Parts marked with `_` are substituted
/// with the provided variables in the order of appearance. We use tail recursion and accumulate
/// (possibly substituted) parts in an accumulator because `query_as!` would not work otherwise;
/// its input must be fully expanded.
#[macro_export]
macro_rules! interpolate_query {
    // Terminal clause: we have a final substitution.
    (query_type: $query_type:ty; acc: $acc:expr; args: $($args:expr,)*; (_,) => $var:literal,) => {
        sqlx::query_as!($query_type, $acc + $var, $($args,)*)
    };
    // Terminal clause: we have a final query part.
    (query_type: $query_type:ty; acc: $acc:expr; args: $($args:expr,)*; ($part:literal,) =>) => {
        sqlx::query_as!($query_type, $acc + $part, $($args,)*)
    };

    // We have a non-terminal substitution. Substitute it with a `var`, add to the accumulator and recurse.
    (
        query_type: $query_type:ty;
        acc: $acc:expr;
        args: $($args:expr,)*;
        (_, $($other_parts:tt,)+) => $var:literal, $($other_vars:literal,)*
    ) => {
        interpolate_query!(
            query_type: $query_type;
            acc: $acc + $var;
            args: $($args,)*;
            ($($other_parts,)+) => $($other_vars,)*
        )
    };
    // We have a non-terminal query part. Add it to the accumulator and recurse.
    (
        query_type: $query_type:ty;
        acc: $acc:expr;
        args: $($args:expr,)*;
        ($part:tt, $($other_parts:tt,)+) => $($vars:literal,)*
    ) => {
        interpolate_query!(
            query_type: $query_type;
            acc: $acc + $part;
            args: $($args,)*;
            ($($other_parts,)+) => $($vars,)*
        )
    };
}

/// Builds a set of statically compiled DB queries based on the provided condition. This allows to avoid copying similar queries
/// or making them dynamic (i.e., using `sqlx::query()` function etc.).
///
/// The macro accepts 3 arguments:
///
/// - Output type. Has same semantics as the type in `sqlx::query_as!` macro.
/// - Query parts, enclosed in `[]` brackets. Each part must be either a string literal, or a `_` placeholder.
/// - `match` expression. Each variant hand must return a `()`-enclosed comma-separated list of substitutions for the placeholder
///   query parts (in the order of their appearance in the query parts), then a semicolon `;`, then a list of arguments
///   for the query (may be empty; has same semantics as arguments for `sqlx::query!`). Each substitution must be a string literal.
///   The number of arguments may differ across variants (e.g., one of variants may introduce one or more additional args).
///
/// See the crate code for examples of usage.
#[macro_export]
macro_rules! match_query_as {
    (
        $query_type:ty,
        [$($parts:tt),+],
        match ($input:expr) {
            $($variants:tt)*
        }
    ) => {
        // We parse `variants` recursively and add parsed parts to an accumulator.
        match_query_as!(
            @inner
            query_type: $query_type,
            input: $input,
            query_parts: ($($parts),+),
            acc: ();
            $($variants)*
        )
    };

    // Terminal clause: we've parsed all match variants. Now we need to expand into a `match` expression.
    (
        @inner
        query_type: $query_type:ty,
        input: $input:expr,
        // We surround token trees (`:tt` [designator]) by `()` so that they are delimited. We need token trees in the first place
        // because it's one of the few forms of designators than can be matched against specific tokens or parsed further (e.g.,
        // as `expr`essions, `pat`terns etc.) during query expansion.
        //
        // [designator]: https://doc.rust-lang.org/rust-by-example/macros/designators.html
        query_parts: ($($_parts:tt),+), // not used; parts are copied into each variant expansion
        acc: ($($acc:tt)*);
    ) => {
        match_query_as!(
            @expand
            query_type: $query_type,
            input: $input,
            acc: ($($acc)*)
        )
    };
    // Non-terminal clause: we have at least one variant left. We add the variant to the accumulator copying parts
    // into it. Copying is necessary because Rust macros are not able to nest expansions of independently repeated vars
    // (i.e., query parts and variants in this case).
    (
        @inner
        query_type: $query_type:ty,
        input: $input:expr,
        query_parts: ($($parts:tt),+),
        acc: ($($acc:tt)*);
        $p:pat => ($($clause:tt)*),
        $($rest:tt)*
    ) => {
        match_query_as!(
            @inner
            query_type: $query_type,
            input: $input,
            query_parts: ($($parts),+),
            acc: ($($acc)* $p => (parts: $($parts,)+) ($($clause)*));
            $($rest)*
        )
    };
    // Expansion routine: match all variants, each with copied query parts, and expand them as a `match` expression
    // with the corresponding variants.
    (
        @expand
        query_type: $query_type:ty,
        input: $input:expr,
        acc: ($(
            $p:pat => (parts: $($parts:tt,)+) ($($substitutions:literal),+ ; $($args:expr),*)
        )*)
    ) => {
      match ($input) {
            $(
            $p => {
                // We need to specify `query` type (specifically, the 2nd type param in `Map`) so that all `match` variants
                // return the same type.
                let query: sqlx::query::Map<_, fn(_) -> _, _> = interpolate_query!(
                    query_type: $query_type;
                    acc: "";
                    args: $($args,)*;
                    ($($parts,)+) => $($substitutions,)+
                );
                query
            }
            )*
        }
    };
}
