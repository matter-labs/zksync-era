use std::fmt;

use crate::{
    helpers::{extract_option_inner_type, Field},
    labels::{CtxLabel, ParseLabels as _, ResourceOrTask},
};
use quote::quote;
use syn::{DeriveInput, Result};

#[derive(Debug)]
pub enum MacroKind {
    FromContext,
    IntoContext,
}

impl fmt::Display for MacroKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FromContext => write!(f, "FromContext"),
            Self::IntoContext => write!(f, "IntoContext"),
        }
    }
}

/// Parser and renderer for `FromContext` derive macro.
#[derive(Debug)]
pub struct MacroImpl {
    macro_kind: MacroKind,
    ctx: CtxLabel,
    ident: syn::Ident,
    fields: Vec<Field>,
}

impl MacroImpl {
    pub(crate) fn parse(macro_kind: MacroKind, input: DeriveInput) -> Result<Self> {
        let ctx = CtxLabel::parse(&input.attrs)?.unwrap_or_default();
        let ident = input.ident;
        if !input.generics.params.is_empty() {
            return Err(syn::Error::new(
                ident.span(),
                format!("Generics are not supported for `{macro_kind}`"),
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
                _ => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("Only named fields are supported for `{macro_kind}`"),
                    ))
                }
            },
            _ => {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("Only structures are supported for `{macro_kind}`"),
                ))
            }
        };

        Ok(Self {
            macro_kind,
            ctx,
            ident,
            fields,
        })
    }

    pub fn render(self) -> Result<proc_macro2::TokenStream> {
        match self.macro_kind {
            MacroKind::FromContext => self.render_from_context(),
            MacroKind::IntoContext => self.render_into_context(),
        }
    }

    fn render_from_context(self) -> Result<proc_macro2::TokenStream> {
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

    fn render_into_context(self) -> Result<proc_macro2::TokenStream> {
        let ident = self.ident;
        let crate_path = if self.ctx.local {
            quote! { crate }
        } else {
            quote! { zksync_node_framework }
        };
        let mut actions = Vec::new();
        for field in self.fields {
            // Label must be provided.
            let Some(label) = field.label else {
                return Err(syn::Error::new_spanned(
                    field.ident,
                    "All fields must have either `task` or `resource` attribute",
                ));
            };

            let ident = field.ident;
            match label {
                ResourceOrTask::Resource(label) => {
                    // Default label is not allowed here.
                    if label.default {
                        return Err(syn::Error::new_spanned(
                            ident,
                            "`default` attribute is not allowed in `IntoContext` macro",
                        ));
                    }

                    actions.push(quote! {
                        ctx.insert_resource(self.#ident)?;
                    });
                }
                ResourceOrTask::Task(_label) => {
                    actions.push(quote! {
                        ctx.add_task(self.#ident);
                    });
                }
            }
        }

        Ok(quote! {
            impl #crate_path::IntoContext for #ident {
                fn into_context(self, ctx: &mut #crate_path::service::ServiceContext<'_>) -> std::result::Result<(), #crate_path::WiringError> {
                    #(#actions)*
                    Ok(())
                }
            }
        })
    }
}
