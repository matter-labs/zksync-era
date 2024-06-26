use syn::{spanned::Spanned as _, Attribute, Result};

/// Trait allowing to iterate over the attributes and parse the labels.
/// Only supports simple path attributes, like `ctx(local)`.
pub(crate) trait ParseLabels: Sized + Default {
    const ATTR_NAME: &'static str;
    const LABELS: &'static [&'static str];

    fn set_label(&mut self, label: &str);

    fn set_span(&mut self, span: proc_macro2::Span);

    fn parse(attrs: &[Attribute]) -> Result<Option<Self>> {
        let mut self_ = Self::default();

        let mut found = false;
        for attr in attrs {
            if attr.path().is_ident(Self::ATTR_NAME) {
                found = true;
                self_.set_span(attr.span());
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
                    for label in Self::LABELS {
                        if meta.path.is_ident(label) {
                            self_.set_label(label);
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

/// Context label, e.g. `ctx(local)`.
#[derive(Debug, Default)]
pub(crate) struct CtxLabel {
    /// Special attribute that marks the derive as internal.
    /// Alters the path to the trait to be implemented.
    pub(crate) local: bool,
    pub(crate) span: Option<proc_macro2::Span>,
}

impl ParseLabels for CtxLabel {
    const ATTR_NAME: &'static str = "ctx";
    const LABELS: &'static [&'static str] = &["local"];

    fn set_label(&mut self, label: &str) {
        match label {
            "local" => self.local = true,
            _ => unreachable!(),
        }
    }

    fn set_span(&mut self, span: proc_macro2::Span) {
        self.span = Some(span);
    }
}

/// Resource label, e.g. `resource` or `resource(default)`.
#[derive(Debug, Default)]
pub(crate) struct ResourceLabel {
    /// Resource should be retrieved with `get_resource_or_default`.
    pub(crate) default: bool,
    pub(crate) span: Option<proc_macro2::Span>,
}

impl ParseLabels for ResourceLabel {
    const ATTR_NAME: &'static str = "resource";
    const LABELS: &'static [&'static str] = &["default"];

    fn set_label(&mut self, label: &str) {
        match label {
            "default" => self.default = true,
            _ => unreachable!(),
        }
    }

    fn set_span(&mut self, span: proc_macro2::Span) {
        self.span = Some(span);
    }
}

/// Task label, e.g. `task`.
#[derive(Debug, Default)]
pub(crate) struct TaskLabel {
    pub(crate) span: Option<proc_macro2::Span>,
}

impl ParseLabels for TaskLabel {
    const ATTR_NAME: &'static str = "task";
    const LABELS: &'static [&'static str] = &[];

    fn set_label(&mut self, _label: &str) {
        unreachable!("No labels are supported for `task`")
    }

    fn set_span(&mut self, span: proc_macro2::Span) {
        self.span = Some(span);
    }
}

#[derive(Debug)]
pub(crate) enum ResourceOrTask {
    Resource(ResourceLabel),
    Task(TaskLabel),
}

impl ResourceOrTask {
    pub(crate) fn parse(attrs: &[Attribute]) -> Result<Option<Self>> {
        if let Some(resource) = ResourceLabel::parse(attrs)? {
            return Ok(Some(ResourceOrTask::Resource(resource)));
        }
        if let Some(task) = TaskLabel::parse(attrs)? {
            return Ok(Some(ResourceOrTask::Task(task)));
        }
        Ok(None)
    }
}
