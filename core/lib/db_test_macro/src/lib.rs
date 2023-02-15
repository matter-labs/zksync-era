use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Ident, Token,
};

/// Argument that can be supplied to the `db_test` macro to be used in the `zksync_dal` crate.
const DAL_CRATE_MARKER_ARG: &str = "dal_crate";
/// Name of the type that represents the connection pool in DAL.
const TYPE_NAME: &str = "ConnectionPool";

#[derive(Debug)]
struct Args {
    vars: Vec<syn::Ident>,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let vars = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
        Ok(Args {
            vars: vars.into_iter().collect(),
        })
    }
}

fn parse_connection_pool_arg_name(arg: Option<&syn::FnArg>) -> Result<syn::PatIdent, ()> {
    if let Some(syn::FnArg::Typed(arg)) = arg {
        if let syn::Pat::Ident(ident) = arg.pat.as_ref() {
            if let syn::Type::Path(path_type) = arg.ty.as_ref() {
                if path_type.path.is_ident(TYPE_NAME) {
                    return Ok(ident.clone());
                }
            }
        }
    }
    Err(())
}

fn parse_knobs(mut input: syn::ItemFn, inside_dal_crate: bool) -> Result<TokenStream, syn::Error> {
    let sig = &mut input.sig;
    let body = &input.block;
    let attrs = &input.attrs;
    let vis = input.vis;

    if sig.asyncness.is_none() {
        let msg = "the async keyword is missing from the function declaration";
        return Err(syn::Error::new_spanned(sig.fn_token, msg));
    }

    sig.asyncness = None;

    let argument_name = parse_connection_pool_arg_name(sig.inputs.first());
    if sig.inputs.len() != 1 || argument_name.is_err() {
        let msg = format!(
            "the DB test function must take a single argument of type {}",
            TYPE_NAME
        );
        return Err(syn::Error::new_spanned(&sig.inputs, msg));
    }

    // Remove argument, as the test function must not have one.
    sig.inputs.pop();

    // We've checked that argument is OK above.
    let argument_name = argument_name.unwrap();

    let rt = quote! { tokio::runtime::Builder::new_current_thread() };

    let header = quote! {
        #[::core::prelude::v1::test]
    };

    let dal_crate_id = if inside_dal_crate {
        quote! { crate }
    } else {
        quote! { zksync_dal }
    };

    let result = quote! {
        #header
        #(#attrs)*
        #vis #sig {
            use #dal_crate_id::connection::test_pool::Connection;

            #rt
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let mut __connection = #dal_crate_id::connection::TestPool::connect_to_test_db().await;
                    let mut __transaction = __connection.begin().await.unwrap();
                    let mut #argument_name = unsafe {
                        #dal_crate_id::ConnectionPool::Test(#dal_crate_id::connection::TestPool::new(&mut __transaction).await)
                    };

                    {
                        #body
                    }
                })
        }
    };

    Ok(result.into())
}

#[proc_macro_attribute]
pub fn db_test(raw_args: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(raw_args as Args);

    // There may be only one argument, and it should match the exact expected value.
    if args.vars.len() > 1 || (args.vars.len() == 1 && args.vars[0] != DAL_CRATE_MARKER_ARG) {
        let msg = format!("only '{DAL_CRATE_MARKER_ARG}' argument is supported");
        return syn::Error::new_spanned(&args.vars[0], msg)
            .to_compile_error()
            .into();
    }
    let inside_dal_crate = args
        .vars
        .first()
        .map(|arg| arg == DAL_CRATE_MARKER_ARG)
        .unwrap_or(false);

    for attr in &input.attrs {
        if attr.path.is_ident("test") {
            let msg = "second test attribute is supplied";
            return syn::Error::new_spanned(attr, msg).to_compile_error().into();
        }
    }

    parse_knobs(input, inside_dal_crate).unwrap_or_else(|e| e.to_compile_error().into())
}
