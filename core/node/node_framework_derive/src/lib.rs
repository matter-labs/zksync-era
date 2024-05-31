extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, LitStr};

#[derive(Debug, Clone, Copy)]
enum Macro {
    Provides,
    Requests,
}

impl Macro {
    fn doc_string(self) -> &'static str {
        match self {
            Macro::Provides => "provides",
            Macro::Requests => "requests",
        }
    }

    fn trait_name(self) -> &'static str {
        match self {
            Macro::Provides => "Provides",
            Macro::Requests => "Requests",
        }
    }

    fn attribute_name(self) -> &'static str {
        match self {
            Macro::Provides => "provides",
            Macro::Requests => "requests",
        }
    }

    fn render(self, input: DeriveInput) -> proc_macro2::TokenStream {
        // Get the struct name
        let name = input.ident.clone();

        // Initialize a vector to hold the types
        let mut provided_types = Vec::new();

        // Whether this derive is called from within the framework crate.
        let mut is_internal = false;

        // Iterate over attributes to find #[provides(..)]
        for attr in &input.attrs {
            if attr.path().is_ident("provides") {
                attr.parse_nested_meta(|meta| {
                    // Special attribute that marks the derive as internal.
                    // Alters the path to the trait to be implemented.
                    if meta.path.is_ident("local") {
                        let value = meta.value().unwrap();
                        let s: LitStr = value.parse().unwrap();
                        if s.value() == "true" {
                            is_internal = true;
                        }
                    } else {
                        provided_types.push(meta.path.clone());
                    }
                    Ok(())
                })
                .unwrap();
            }
        }

        let crate_name = if is_internal {
            quote! { crate }
        } else {
            quote! { zksync_node_framework }
        };

        // Generate the impl blocks for each provided type
        let impls = provided_types.iter().map(|ty| {
            quote! {
                impl #crate_name::service::Provides<#ty> for #name {}
            }
        });

        // Combine everything into the final output
        quote! {
            #(#impls)*
        }
    }
}

#[proc_macro_derive(Provides, attributes(provides))]
pub fn provides_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    Macro::Provides.render(input).into()
}
