//! Errors interacting with the Merkle tree.

use std::{error, fmt, str::Utf8Error};

use crate::types::NodeKey;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DeserializeErrorKind {
    #[error("Tree manifest was expected, but is missing")]
    MissingManifest,
    #[error("Node was expected, but is missing")]
    MissingNode,
    #[error("Unexpected end of input")]
    UnexpectedEof,
    #[error("data left after deserialization")]
    Leftovers,
    /// Error reading a LEB128-encoded value.
    #[error("failed reading LEB128-encoded value: {0}")]
    Leb128(#[source] leb128::read::Error),
    /// Error reading a UTF-8 string.
    #[error("failed reading UTF-8 string: {0}")]
    Utf8(#[source] Utf8Error),

    /// Missing required tag in the tree manifest.
    #[error("missing required tag `{0}` in tree manifest")]
    MissingTag(&'static str),
    /// Unknown tag in the tree manifest.
    #[error("unknown tag `{0}` in tree manifest")]
    UnknownTag(String),
    /// Malformed tag in the tree manifest.
    #[error("malformed tag `{name}` in tree manifest: {err}")]
    MalformedTag {
        /// Tag name.
        name: &'static str,
        /// Error that has occurred parsing the tag.
        #[source]
        err: Box<dyn error::Error + Send + Sync>,
    },
    #[error("storage backend error: {0}")]
    Backend(#[source] anyhow::Error),
}

#[derive(Debug)]
pub(crate) enum DeserializeContext {
    Manifest,
    Node(NodeKey),
    ChildRef(u8),
    LeafCount,
    KeyIndex(Box<[u8]>),
}

impl fmt::Display for DeserializeContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(key) => write!(formatter, "node at {key}"),
            Self::ChildRef(idx) => write!(formatter, "child ref {idx}"),
            Self::LeafCount => write!(formatter, "leaf count"),
            Self::Manifest => write!(formatter, "manifest"),
            Self::KeyIndex(key) => write!(formatter, "key index {key:?}"),
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

impl std::error::Error for DeserializeError {}
