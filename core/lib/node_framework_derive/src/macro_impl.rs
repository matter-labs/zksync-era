use std::fmt;

use quote::quote;
use syn::{DeriveInput, Result};

use crate::{helpers::Field, labels::CtxLabel};

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
                        let label = CtxLabel::parse(&field.attrs)?.unwrap_or_default();
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

    fn crate_path(&self) -> proc_macro2::TokenStream {
        if let Some(krate) = &self.ctx.krate {
            quote! { #krate }
        } else {
            quote! { zksync_node_framework }
        }
    }

    fn render_from_context(self) -> Result<proc_macro2::TokenStream> {
        let crate_path = self.crate_path();
        let ident = self.ident;
        let mut fields = Vec::new();
        for field in self.fields {
            let ty = field.ty;
            let ident = field.ident;
            let default = field.label.default;

            if field.label.krate.is_some() {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`crate` attribute is not allowed for fields",
                ));
            }

            if field.label.task {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`task` attribute is not allowed in `FromContext` macro",
                ));
            }

            let field = if default {
                quote! {
                    #ident: ctx.get_resource_or_default::<#ty>()
                }
            } else {
                quote! {
                    #ident: <#ty as #crate_path::service::FromContext>::from_context(ctx)?
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
        let crate_path = self.crate_path();
        let ident = self.ident;
        let mut actions = Vec::new();
        for field in self.fields {
            let ty = field.ty;
            let ident = field.ident;
            if field.label.default {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`default` attribute is not allowed in `IntoContext` macro",
                ));
            }

            if field.label.krate.is_some() {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`crate` attribute is not allowed for fields",
                ));
            }

            let action = if field.label.task {
                // Check whether the task is an `Option`.
                if let Some(_inner_ty) = crate::helpers::extract_option_inner_type(&ty) {
                    quote! {
                        if let Some(task) = self.#ident {
                            ctx.add_task(task);
                        }
                    }
                } else {
                    quote! {
                        ctx.add_task(self.#ident);
                    }
                }
            } else {
                quote! {
                    <#ty as #crate_path::service::IntoContext>::into_context(self.#ident, ctx)?;
                }
            };
            actions.push(action);
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
