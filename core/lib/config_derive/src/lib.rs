#![recursion_limit = "128"]

extern crate proc_macro;

use proc_macro::TokenStream;

mod config;

#[proc_macro_derive(DescribeConfig)]
pub fn config(input: TokenStream) -> TokenStream {
    config::impl_describe_config(input)
}
