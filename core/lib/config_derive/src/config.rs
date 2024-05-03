//! `DescribeConfig` derive macro implementation.

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, quote_spanned};
use syn::{
    spanned::Spanned, Attribute, Data, DeriveInput, Expr, Field, GenericArgument, Lit, LitStr,
    Path, PathArguments, Type, TypePath,
};

fn parse_docs(attrs: &[Attribute]) -> String {
    let doc_lines = attrs.iter().filter_map(|attr| {
        if attr.meta.path().is_ident("doc") {
            let name_value = attr.meta.require_name_value().ok()?;
            let Expr::Lit(doc_literal) = &name_value.value else {
                return None;
            };
            match &doc_literal.lit {
                Lit::Str(doc_literal) => Some(doc_literal.value()),
                _ => None,
            }
        } else {
            None
        }
    });

    let mut docs = String::new();
    for line in doc_lines {
        let line = line.trim();
        if line.is_empty() {
            if !docs.is_empty() {
                // New paragraph; convert it to a new line.
                docs.push('\n');
            }
        } else {
            if !docs.is_empty() && !docs.ends_with(|ch: char| ch.is_ascii_whitespace()) {
                docs.push(' ');
            }
            docs.push_str(line);
        }
    }
    docs
}

/// Recognized subset of `serde` field attributes.
#[derive(Debug)]
struct SerdeData {
    rename: Option<String>,
    aliases: Vec<String>,
    default: Option<Option<Path>>,
}

impl SerdeData {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let serde_attrs = attrs.iter().filter(|attr| attr.path().is_ident("serde"));
        let mut rename = None;
        let mut aliases = vec![];
        let mut default = None;
        for attr in serde_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    let s: LitStr = meta.value()?.parse()?;
                    rename = Some(s.value());
                } else if meta.path.is_ident("alias") {
                    let s: LitStr = meta.value()?.parse()?;
                    aliases.push(s.value());
                } else if meta.path.is_ident("default") {
                    if meta.input.is_empty() {
                        default = Some(None);
                    } else {
                        let s: LitStr = meta.value()?.parse()?;
                        default = Some(Some(s.parse()?));
                    }
                } else {
                    // Digest any tokens
                    meta.input.parse::<proc_macro2::TokenStream>()?;
                }
                Ok(())
            })?;
        }
        Ok(Self {
            rename,
            aliases,
            default,
        })
    }
}

struct ConfigField {
    name: Ident,
    ty: Type,
    docs: String,
    serde_data: SerdeData,
}

impl ConfigField {
    fn new(raw: &Field) -> syn::Result<Self> {
        let name = raw.ident.clone().ok_or_else(|| {
            let message = "Only named fields are supported";
            syn::Error::new_spanned(raw, message)
        })?;
        let ty = raw.ty.clone();

        let serde_data = SerdeData::new(&raw.attrs)?;
        Ok(Self {
            name,
            ty,
            docs: parse_docs(&raw.attrs),
            serde_data,
        })
    }

    fn extract_base_type(mut ty: &Type) -> &Type {
        loop {
            ty = match ty {
                Type::Array(array) => array.elem.as_ref(),
                Type::Path(TypePath { path, .. }) => {
                    if path.segments.len() != 1 {
                        break;
                    }
                    let segment = &path.segments[0];
                    if segment.ident != "Vec" && segment.ident != "Option" {
                        break;
                    }
                    let PathArguments::AngleBracketed(angle_bracketed) = &segment.arguments else {
                        break;
                    };
                    if angle_bracketed.args.len() != 1 {
                        break;
                    }
                    match &angle_bracketed.args[0] {
                        GenericArgument::Type(ty) => ty,
                        _ => break,
                    }
                }
                _ => break,
            };
        }
        ty
    }

    fn is_option(ty: &Type) -> bool {
        let Type::Path(TypePath { path, .. }) = ty else {
            return false;
        };
        if path.segments.len() != 1 {
            return false;
        }
        let segment = &path.segments[0];
        if segment.ident != "Option" {
            return false;
        }
        let PathArguments::AngleBracketed(angle_bracketed) = &segment.arguments else {
            return false;
        };
        angle_bracketed.args.len() == 1
    }

    fn describe(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let name = &self.name;
        let name_str = self
            .serde_data
            .rename
            .clone()
            .unwrap_or_else(|| name.to_string());
        let aliases = self.serde_data.aliases.iter();
        let help = &self.docs;

        let ty = &self.ty;
        let ty_in_code = if let Some(text) = ty.span().source_text() {
            quote!(#text)
        } else {
            quote!(::core::stringify!(#ty))
        };
        let base_type = Self::extract_base_type(&self.ty);
        let base_type_in_code = if let Some(text) = base_type.span().source_text() {
            quote!(#text)
        } else {
            quote!(::core::stringify!(#base_type))
        };

        let default_value = match &self.serde_data.default {
            None if !Self::is_option(ty) => None,
            Some(None) | None => Some(quote_spanned! {name.span()=>
                <::std::boxed::Box<#ty> as ::core::default::Default>::default()
            }),
            Some(Some(path)) => {
                Some(quote_spanned!(name.span()=> ::std::boxed::Box::<#ty>::new(#path())))
            }
        };
        let default_value = if let Some(value) = default_value {
            quote_spanned!(name.span()=> ::core::option::Option::Some(|| #value))
        } else {
            quote_spanned!(name.span()=> ::core::option::Option::None)
        };

        quote_spanned! {name.span()=> {
            let base_type = #cr::RustType::of::<#base_type>(#base_type_in_code);
            #cr::ParamMetadata {
                name: #name_str,
                aliases: &[#(#aliases,)*],
                help: #help,
                ty: #cr::RustType::of::<#ty>(#ty_in_code),
                base_type,
                unit: #cr::UnitOfMeasurement::detect(#name_str, base_type),
                default_value: #default_value,
            }
        }}
    }
}

struct DescribeConfigAttrs {
    cr: Option<Path>,
}

impl DescribeConfigAttrs {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));
        let mut cr = None;
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") {
                    let path: Path = meta.value()?.parse()?;
                    cr = Some(path);
                    Ok(())
                } else {
                    Err(meta.error("unsupported attribute; only `crate` is supported"))
                }
            })?;
        }
        Ok(Self { cr })
    }
}

struct DescribeConfigImpl {
    attrs: DescribeConfigAttrs,
    name: Ident,
    help: String,
    fields: Vec<ConfigField>,
}

impl DescribeConfigImpl {
    fn new(raw: &DeriveInput) -> syn::Result<Self> {
        let Data::Struct(data) = &raw.data else {
            let message = "#[derive(DescribeConfig)] can only be placed on structs";
            return Err(syn::Error::new_spanned(raw, message));
        };
        if raw.generics.type_params().count() != 0
            || raw.generics.const_params().count() != 0
            || raw.generics.lifetimes().count() != 0
        {
            let message = "generics are not supported";
            return Err(syn::Error::new_spanned(&raw.generics, message));
        }

        let attrs = DescribeConfigAttrs::new(&raw.attrs)?;
        let name = raw.ident.clone();
        let fields = data
            .fields
            .iter()
            .map(ConfigField::new)
            .collect::<syn::Result<_>>()?;
        Ok(Self {
            attrs,
            name,
            help: parse_docs(&raw.attrs),
            fields,
        })
    }

    fn cr(&self) -> proc_macro2::TokenStream {
        if let Some(cr) = &self.attrs.cr {
            quote!(#cr::metadata)
        } else {
            let span = self.name.span();
            quote_spanned!(span=> ::zksync_config::metadata)
        }
    }

    fn derive_describe_config(&self) -> proc_macro2::TokenStream {
        let cr = self.cr();
        let name = &self.name;
        let name_str = name.to_string();
        let help = &self.help;
        let params = self.fields.iter().map(|field| field.describe(&cr));

        quote! {
            impl #cr::DescribeConfig for #name {
                fn describe_config() -> &'static #cr::ConfigMetadata {
                    static METADATA_CELL: #cr::Lazy<#cr::ConfigMetadata> = #cr::Lazy::new(|| #cr::ConfigMetadata {
                        ty: #cr::RustType::of::<#name>(#name_str),
                        help: #help,
                        params: ::std::boxed::Box::new([#(#params,)*]),
                    });
                    &METADATA_CELL
                }
            }
        }
    }
}

pub(crate) fn impl_describe_config(input: TokenStream) -> TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let trait_impl = match DescribeConfigImpl::new(&input) {
        Ok(trait_impl) => trait_impl,
        Err(err) => return err.into_compile_error().into(),
    };
    trait_impl.derive_describe_config().into()
}
