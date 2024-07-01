use std::fmt;

use syn::{GenericArgument, PathArguments, Type};

use crate::labels::CtxLabel;

/// Representation of a single structure field.
pub(crate) struct Field {
    /// Name of the field.
    pub(crate) ident: syn::Ident,
    /// Type of the field.
    pub(crate) ty: syn::Type,
    /// Parsed label.
    pub(crate) label: CtxLabel,
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Field")
            .field("ident", &self.ident)
            .field("label", &self.label)
            .finish()
    }
}

// Helper function to check if a field is of type Option<T> and extract T
pub(crate) fn extract_option_inner_type(ty: &Type) -> Option<&Type> {
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
