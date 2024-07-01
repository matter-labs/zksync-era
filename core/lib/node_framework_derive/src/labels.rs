use std::fmt;

use syn::{spanned::Spanned as _, Attribute, Result};

/// Context label, e.g. `ctx(crate = "crate")`.
#[derive(Default)]
pub(crate) struct CtxLabel {
    /// Special attribute that marks the derive as internal.
    /// Alters the path to the trait to be implemented.
    pub(crate) krate: Option<syn::Path>, // `crate` is a reserved keyword and cannot be a raw identifier.
    pub(crate) span: Option<proc_macro2::Span>,
    pub(crate) task: bool,
    pub(crate) default: bool,
}

impl fmt::Debug for CtxLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For some weird reason, doc tests fail with the derived impl, stating that
        // `syn::Path` does not implement `Debug`.
        f.debug_struct("CtxLabel")
            .field("krate", &self.krate.as_ref().and_then(|p| p.get_ident()))
            .field("span", &self.span)
            .field("task", &self.task)
            .field("default", &self.default)
            .finish()
    }
}

impl CtxLabel {
    const ATTR_NAME: &'static str = "context";
    const CRATE_LABEL: &'static str = "crate";
    const TASK_LABEL: &'static str = "task";
    const DEFAULT_LABEL: &'static str = "default";
    const LABELS: &'static [&'static str] =
        &[Self::CRATE_LABEL, Self::TASK_LABEL, Self::DEFAULT_LABEL];

    pub(crate) fn parse(attrs: &[Attribute]) -> Result<Option<Self>> {
        let mut self_ = Self::default();

        let mut found = false;
        for attr in attrs {
            if attr.path().is_ident(Self::ATTR_NAME) {
                found = true;
                self_.span = Some(attr.span());
                match attr.meta {
                    syn::Meta::Path(_) => {
                        // No values to parse.
                        break;
                    }
                    syn::Meta::NameValue(_) => {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "Unexpected value, expected a list of labels",
                        ));
                    }
                    syn::Meta::List(_) => {
                        // Do nothing, parsing happens below.
                    }
                }
                attr.parse_nested_meta(|meta| {
                    let mut added = false;
                    for &label in Self::LABELS {
                        if meta.path.is_ident(label) {
                            match label {
                                Self::CRATE_LABEL => {
                                    let value = meta.value()?;
                                    let lit: syn::LitStr = value.parse()?; // Check that the value is a path.
                                    self_.krate = Some(syn::parse_str(&lit.value())?);
                                }
                                Self::TASK_LABEL => {
                                    self_.task = true;
                                }
                                Self::DEFAULT_LABEL => {
                                    self_.default = true;
                                }
                                _ => unreachable!(),
                            }
                            added = true;
                            break;
                        }
                    }

                    if !added {
                        let err_msg =
                            format!("Unexpected token, supported labels: `{:?}`", Self::LABELS);
                        let err = syn::Error::new_spanned(attr, err_msg);
                        return Err(err);
                    }
                    Ok(())
                })?;
            }
        }
        if !found {
            return Ok(None);
        }
        Ok(Some(self_))
    }
}
