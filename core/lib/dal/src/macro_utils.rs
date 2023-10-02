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
