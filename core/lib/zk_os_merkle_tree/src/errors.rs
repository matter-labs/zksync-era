//! Errors interacting with the Merkle tree.

use std::fmt;

use crate::types::NodeKey;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DeserializeErrorKind {
    #[error("Node was expected, but is missing")]
    MissingNode,
}

#[derive(Debug)]
pub(crate) enum DeserializeContext {
    Node(NodeKey),
}

impl fmt::Display for DeserializeContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(key) => write!(formatter, "node at {key}"),
        }
    }
}

/// Error that can occur during deserialization.
#[derive(Debug)]
pub struct DeserializeError {
    kind: DeserializeErrorKind,
    contexts: Vec<DeserializeContext>,
}

impl From<DeserializeErrorKind> for DeserializeError {
    fn from(kind: DeserializeErrorKind) -> Self {
        Self {
            kind,
            contexts: vec![],
        }
    }
}

impl DeserializeError {
    #[must_use]
    pub(crate) fn with_context(mut self, context: DeserializeContext) -> Self {
        self.contexts.push(context);
        self
    }
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `self.contexts` are ordered from the most specific one to the most general one
        if !self.contexts.is_empty() {
            write!(formatter, "[in ")?;
            for (i, context) in self.contexts.iter().enumerate() {
                write!(formatter, "{context}")?;
                if i + 1 < self.contexts.len() {
                    write!(formatter, ", ")?;
                }
            }
            write!(formatter, "] ")?;
        }
        write!(formatter, "{}", self.kind)
    }
}
