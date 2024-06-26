extern crate proc_macro;

use std::fmt;

use labels::{CtxLabel, ParseLabels as _, ResourceOrTask};
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, GenericArgument, PathArguments, Result, Type};

mod labels;

/// Representation of a single structure field.
struct Field {
    /// Name of the field.
    ident: syn::Ident,
    /// Type of the field.
    ty: syn::Type,
    /// `resource`/`task` label.
    label: Option<ResourceOrTask>,
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Field")
            .field("ident", &self.ident)
            .field("label", &self.label)
            .finish()
    }
}

#[derive(Debug)]
struct FromContext {
    ctx: CtxLabel,
    ident: syn::Ident,
    fields: Vec<Field>,
}

// Helper function to check if a field is of type Option<T> and extract T
fn extract_option_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        // Check if the path is `Option`
        if type_path.path.segments.len() == 1 {
            let segment = &type_path.path.segments[0];
            if segment.ident == "Option" {
                if let PathArguments::AngleBracketed(angle_bracketed_args) = &segment.arguments {
                    if angle_bracketed_args.args.len() == 1 {
                        if let GenericArgument::Type(inner_type) = &angle_bracketed_args.args[0] {
                            return Some(inner_type);
                        }
                    }
                }
            }
        }
    }
    None
}

impl FromContext {
    fn parse(input: DeriveInput) -> Result<Self> {
        let ctx = CtxLabel::parse(&input.attrs)?.unwrap_or_default();
        let ident = input.ident;
        if !input.generics.params.is_empty() {
            return Err(syn::Error::new(
                ident.span(),
                "Generics are not supported for `FromContext`",
            ));
        }
        let fields = match input.data {
            syn::Data::Struct(data) => match data.fields {
                syn::Fields::Named(fields) => fields
                    .named
                    .into_iter()
                    .map(|field| {
                        let ident = field.ident.unwrap();
                        let ty = field.ty;
                        let label = ResourceOrTask::parse(&field.attrs)?;
                        Ok(Field { ident, ty, label })
                    })
                    .collect::<Result<Vec<_>>>()?,
                _ => unimplemented!(),
            },
            _ => unimplemented!(),
        };

        Ok(Self { ctx, ident, fields })
    }

    fn render(self) -> Result<proc_macro2::TokenStream> {
        let ident = self.ident;
        let crate_path = if self.ctx.local {
            quote! { crate }
        } else {
            quote! { zksync_node_framework }
        };
        let mut fields = Vec::new();
        for field in self.fields {
            // Only accept `resource` labels here.
            let label = match field.label {
                Some(ResourceOrTask::Resource(label)) => Some(label),
                Some(ResourceOrTask::Task(task)) => {
                    return Err(syn::Error::new(
                        task.span.unwrap(), // Must be set during parsing.
                        "Only `resource` labels are supported for `FromContext`",
                    ));
                }
                None => None,
            };

            let mut ty = field.ty;

            // Type may be an option, and it has special handling.
            // Try to extract the wrapped type.
            let mut is_option = false;
            if let Some(wrapped_ty) = extract_option_inner_type(&ty) {
                ty = wrapped_ty.clone();
                is_option = true;
            };

            let ident = field.ident;
            let default = label.map(|label| label.default).unwrap_or(false);

            let field = match (is_option, default) {
                (false, false) => quote! {
                    #ident: ctx.get_resource::<#ty>()?
                },
                (false, true) => quote! {
                    #ident: ctx.get_resource_or_default::<#ty>()?
                },
                (true, false) => quote! {
                    #ident: match ctx.get_resource::<#ty>() {
                        Ok(resource) => Some(resource),
                        Err(#crate_path::WiringError::ResourceLacking { .. }) => None,
                        Err(err) => return Err(err),
                    }
                },
                (true, true) => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "`default` cannot be used on `Option` types",
                    ));
                }
            };

            fields.push(field)
        }

        Ok(quote! {
            impl #crate_path::FromContext for #ident {
                fn from_context(ctx: &mut #crate_path::service::ServiceContext<'_>) -> std::result::Result<Self, #crate_path::WiringError> {
                    Ok(Self {
                        #(#fields),*
                    })
                }
            }
        })
    }
}

#[proc_macro_derive(FromContext, attributes(ctx, resource, task))]
pub fn provides_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    FromContext::parse(input)
        .and_then(|from_context| from_context.render())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
