//! Miscellaneous helper macros.

/// Writes to a [`String`]. This is equivalent to `write!`, but without the need to `unwrap()` the result.
macro_rules! write_str {
    ($buffer:expr, $($args:tt)+) => {{
        use std::fmt::Write as _;
        let __buffer: &mut std::string::String = $buffer;
        std::write!(__buffer, $($args)+).unwrap(); // Writing to a string cannot result in an error
    }};
}

/// Writing a line to a [`String`]. This is equivalent to `writeln!`, but without the need
/// to `unwrap()` the result.
macro_rules! writeln_str {
    ($buffer:expr, $($args:tt)+) => {{
        use std::fmt::Write as _;
        let __buffer: &mut std::string::String = $buffer;
        std::writeln!(__buffer, $($args)+).unwrap(); // Writing to a string cannot result in an error
    }};
}

macro_rules! interpolate_query {
    ($query_type:ty; $acc:expr; $($args:expr,)*; (_,) => $var:literal,) => {
        sqlx::query_as!($query_type, $acc + $var, $($args,)*)
    };
    ($query_type:ty; $acc:expr; $($args:expr,)*; ($part:literal,) =>) => {
        sqlx::query_as!($query_type, $acc + $part, $($args,)*)
    };
    ($query_type:ty; $acc:expr; $($args:expr,)*; (_, $($other_parts:tt,)+) => $var:literal, $($other_vars:literal,)*) => {
        interpolate_query!(
            $query_type;
            $acc + $var;
            $($args,)*;
            ($($other_parts,)+) => $($other_vars,)*
        )
    };
    ($query_type:ty; $acc:expr; $($args:expr,)*; ($part:tt, $($other_parts:tt,)+) => $($vars:literal,)*) => {
        interpolate_query!(
            $query_type;
            $acc + $part;
            $($args,)*;
            ($($other_parts,)+) => $($vars,)*
        )
    };
}

macro_rules! match_query_as {
    (
        $query_type:ty,
        [$($parts:tt),+],
        match ($input:expr) {
            $($variants:tt)*
        }
    ) => {
        match_query_as!(
            @inner
            query_type: $query_type,
            input: $input,
            query_parts: ($($parts),+),
            acc: ();
            $($variants)*
        )
    };

    (
        @inner
        query_type: $query_type:ty,
        input: $input:expr,
        query_parts: ($($_parts:tt),+), // not used; parts are inlined into each clause
        acc: ($($acc:tt)*);
    ) => {
        match_query_as!(
            @expand
            query_type: $query_type,
            input: $input,
            acc: ($($acc)*)
        )
    };
    (
        @inner
        query_type: $query_type:ty,
        input: $input:expr,
        query_parts: ($($parts:tt),+),
        acc: ($($acc:tt)*);
        $p:pat => ($($clause:tt)*)
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
                let query: sqlx::query::Map<_, fn(_) -> _, _> = interpolate_query!(
                    $query_type;
                    "";
                    $($args,)*;
                    ($($parts,)+) => $($substitutions,)+
                );
                query
            }
            )*
        }
    };
}
