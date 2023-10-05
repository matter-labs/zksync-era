//! Error types.

use std::{error, fmt, str::Utf8Error};

use crate::types::NodeKey;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DeserializeErrorKind {
    /// Unexpected end-of-input was encountered.
    #[error("unexpected end of input")]
    UnexpectedEof,
    /// Error reading a LEB128-encoded value.
    #[error("failed reading LEB128-encoded value: {0}")]
    Leb128(#[source] leb128::read::Error),
    /// Error reading a UTF-8 string.
    #[error("failed reading UTF-8 string: {0}")]
    Utf8(#[source] Utf8Error),
    /// An internal node is empty (has no children).
    #[error("empty internal node")]
    EmptyInternalNode,
    /// Bit mask specifying a child kind in an internal tree node is invalid.
    #[error("invalid bit mask specifying a child kind in an internal tree node")]
    InvalidChildKind,

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
}

impl DeserializeErrorKind {
    /// Appends a context to this error.
    pub fn with_context(self, context: ErrorContext) -> DeserializeError {
        DeserializeError {
            kind: self,
            contexts: vec![context],
        }
    }
}

/// Context in which a [`DeserializeError`] can occur.
#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorContext {
    /// Tree manifest.
    Manifest,
    /// Root with the specified version.
    Root(u64),
    /// Leaf node with the specified storage key.
    Leaf(NodeKey),
    /// Internal node with the specified storage key.
    InternalNode(NodeKey),
    /// Hash value of a child reference in an internal tree node.
    ChildRefHash,
    /// Mask in an internal node specifying children existence and type.
    ChildrenMask,

    /// Number of leaf nodes in a tree root.
    LeafCount,
    /// Leaf index in a leaf node.
    LeafIndex,
    /// Version of a child in an internal node.
    Version,
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest => formatter.write_str("tree manifest"),
            Self::Root(version) => write!(formatter, "root at version {version}"),
            Self::Leaf(key) => write!(formatter, "leaf at `{key}`"),
            Self::InternalNode(key) => write!(formatter, "internal node at `{key}`"),
            Self::ChildRefHash => formatter.write_str("hash value of a child reference"),
            Self::ChildrenMask => formatter.write_str("children mask"),
            Self::LeafCount => formatter.write_str("number of leaf nodes"),
            Self::LeafIndex => formatter.write_str("leaf index"),
            Self::Version => formatter.write_str("version of a child"),
        }
    }
}

/// Error deserializing data from RocksDB.
#[derive(Debug)]
pub struct DeserializeError {
    kind: DeserializeErrorKind,
    contexts: Vec<ErrorContext>,
}

impl From<DeserializeErrorKind> for DeserializeError {
    fn from(kind: DeserializeErrorKind) -> Self {
        Self {
            kind,
            contexts: Vec::new(),
        }
    }
}

impl DeserializeError {
    /// Appends a context to this error.
    #[must_use]
    pub fn with_context(mut self, context: ErrorContext) -> Self {
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

impl error::Error for DeserializeError {}

/// Error accessing a specific tree version.
#[derive(Debug)]
pub struct NoVersionError {
    pub(crate) missing_version: u64,
    pub(crate) version_count: u64,
}

impl fmt::Display for NoVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let &Self {
            missing_version,
            version_count,
        } = self;
        if missing_version >= version_count {
            write!(
                formatter,
                "Version {missing_version} does not exist in Merkle tree; it has {version_count} versions"
            )
        } else {
            write!(
                formatter,
                "Version {missing_version} was pruned from Merkle tree"
            )
        }
    }
}

impl error::Error for NoVersionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{types::Nibbles, Key};
    use zksync_types::U256;

    const TEST_KEY: Key = U256([0, 0, 0, 0x_dead_beef_0000_0000]);

    #[test]
    fn displaying_deserialize_error_for_leaf() {
        let err = leb128::read::Error::Overflow;
        let key = Nibbles::new(&TEST_KEY, 5).with_version(5);
        let err = DeserializeErrorKind::Leb128(err)
            .with_context(ErrorContext::LeafIndex)
            .with_context(ErrorContext::Leaf(key));
        let err = err.to_string();

        assert!(
            err.starts_with("[in leaf index, leaf at `5:deadb`]"),
            "{err}"
        );
        assert!(err.contains("failed reading LEB128-encoded value"), "{err}");
    }

    #[test]
    fn displaying_deserialize_error_for_internal_node() {
        let key = Nibbles::new(&TEST_KEY, 4).with_version(7);
        let err = DeserializeErrorKind::UnexpectedEof
            .with_context(ErrorContext::ChildrenMask)
            .with_context(ErrorContext::InternalNode(key));
        let err = err.to_string();

        assert!(
            err.starts_with("[in children mask, internal node at `7:dead`]"),
            "{err}"
        );
        assert!(err.ends_with("unexpected end of input"), "{err}");
    }
}
