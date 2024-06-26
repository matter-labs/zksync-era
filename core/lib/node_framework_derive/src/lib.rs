extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

use crate::macro_impl::{MacroImpl, MacroKind};

pub(crate) mod helpers;
mod labels;
mod macro_impl;

#[proc_macro_derive(FromContext, attributes(ctx, resource, task))]
pub fn from_context_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    MacroImpl::parse(MacroKind::FromContext, input)
        .and_then(|from_context| from_context.render())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(IntoContext, attributes(ctx, resource, task))]
pub fn into_context_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    MacroImpl::parse(MacroKind::IntoContext, input)
        .and_then(|from_context| from_context.render())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
