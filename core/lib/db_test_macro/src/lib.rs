use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    FnArg, Ident, Token,
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

    let rt = quote! { tokio::runtime::Builder::new_current_thread() };
    let header = quote! {
        #[::core::prelude::v1::test]
    };
    let dal_crate_id = if inside_dal_crate {
        quote! { crate }
    } else {
        quote! { zksync_dal }
    };

    let Some(pool_arg) = sig.inputs.pop() else {
        let msg = format!(
            "DB test function must take one or two arguments of type `{}`",
            TYPE_NAME
        );
        return Err(syn::Error::new_spanned(&sig.inputs, msg));
    };

    let FnArg::Typed(pool_arg) = pool_arg.value() else {
        let msg = "Pool argument must be typed";
        return Err(syn::Error::new_spanned(&pool_arg, msg));
    };
    let main_pool_arg_name = &pool_arg.pat;
    let main_pool_arg_type = &pool_arg.ty;

    let prover_pool_arg = sig.inputs.pop();
    let pools_tokens = if let Some(pool_arg) = prover_pool_arg {
        let FnArg::Typed(prover_pool_arg) = pool_arg.value() else {
            let msg = "Pool argument must be typed";
            return Err(syn::Error::new_spanned(&pool_arg, msg));
        };
        let prover_pool_arg_name = &prover_pool_arg.pat;
        let prover_pool_arg_type = &prover_pool_arg.ty;
        quote! {
            let __test_main_pool = #dal_crate_id::connection::TestPool::new().await;
            let __test_prover_pool = #dal_crate_id::connection::TestPool::new().await;
            let #main_pool_arg_name: #main_pool_arg_type = #dal_crate_id::ConnectionPool::Test(__test_main_pool);
            let #prover_pool_arg_name: #prover_pool_arg_type = #dal_crate_id::ConnectionPool::Test(__test_prover_pool);
        }
    } else {
        quote! {
            let __test_main_pool = #dal_crate_id::connection::TestPool::new().await;
            let #main_pool_arg_name: #main_pool_arg_type = #dal_crate_id::ConnectionPool::Test(__test_main_pool);
        }
    };
    let result = quote! {
        #header
        #(#attrs)*
        #vis #sig {
            #rt.enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    #pools_tokens
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
