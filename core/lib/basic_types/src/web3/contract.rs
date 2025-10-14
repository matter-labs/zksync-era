//! Serialization logic allowing to convert between [`ethabi::Token`]s and domain types, such as `H256`,
//! `U256` and `Address`.
//!
//! The majority of the code is copied from the `web3` crate 0.19.0, https://github.com/tomusdrw/rust-web3,
//! licensed under the MIT open-source license.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid output type: {0}")]
    InvalidOutputType(String),
    #[error("{0}")]
    Other(String),
}

use ethabi::{Token, Token::Uint};

use crate::{H160, H256, U256};

pub trait Detokenize: Sized {
    fn from_tokens(tokens: Vec<ethabi::Token>) -> Result<Self, Error>;
}

impl<T: Tokenizable> Detokenize for T {
    fn from_tokens(mut tokens: Vec<ethabi::Token>) -> Result<Self, Error> {
        if tokens.len() != 1 {
            return Err(Error::InvalidOutputType(format!(
                "expected array with 1 token, got {tokens:?}"
            )));
        }
        Self::from_token(tokens.pop().unwrap())
    }
}

pub trait Tokenize {
    fn into_tokens(self) -> Vec<ethabi::Token>;
}

impl<T: Tokenizable> Tokenize for T {
    fn into_tokens(self) -> Vec<ethabi::Token> {
        vec![self.into_token()]
    }
}

impl Tokenize for () {
    fn into_tokens(self) -> Vec<ethabi::Token> {
        vec![]
    }
}

macro_rules! impl_tokenize_for_tuple {
    ($($idx:tt : $ty:ident),+) => {
        impl<$($ty,)+> Tokenize for ($($ty,)+)
        where
            $($ty : Tokenizable,)+
        {
            fn into_tokens(self) -> Vec<ethabi::Token> {
                vec![$(self.$idx.into_token(),)+]
            }
        }
    };
}

impl_tokenize_for_tuple!(0: A);
impl_tokenize_for_tuple!(0: A, 1: B);
impl_tokenize_for_tuple!(0: A, 1: B, 2: C);
impl_tokenize_for_tuple!(0: A, 1: B, 2: C, 3: D);
impl_tokenize_for_tuple!(0: A, 1: B, 2: C, 3: D, 4: E);
impl_tokenize_for_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F);
impl_tokenize_for_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G);
impl_tokenize_for_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H);

pub trait Tokenizable: Sized {
    fn from_token(token: ethabi::Token) -> Result<Self, Error>;
    fn into_token(self) -> ethabi::Token;
}

impl Tokenizable for bool {
    fn from_token(token: ethabi::Token) -> Result<Self, Error> {
        match token {
            ethabi::Token::Bool(flag) => Ok(flag),
            _ => Err(Error::InvalidOutputType(format!(
                "expected Boolean, got {token:?}"
            ))),
        }
    }

    fn into_token(self) -> ethabi::Token {
        ethabi::Token::Bool(self)
    }
}

impl Tokenizable for H160 {
    fn from_token(token: ethabi::Token) -> Result<Self, Error> {
        match token {
            ethabi::Token::Address(address) => Ok(address),
            _ => Err(Error::InvalidOutputType(format!(
                "expected address, got {token:?}"
            ))),
        }
    }

    fn into_token(self) -> ethabi::Token {
        ethabi::Token::Address(self)
    }
}

impl Tokenizable for U256 {
    fn from_token(token: ethabi::Token) -> Result<Self, Error> {
        match token {
            ethabi::Token::Uint(value) => Ok(value),
            _ => Err(Error::InvalidOutputType(format!(
                "expected uint256, got {token:?}"
            ))),
        }
    }

    fn into_token(self) -> ethabi::Token {
        ethabi::Token::Uint(self)
    }
}

impl Tokenizable for H256 {
    fn from_token(token: ethabi::Token) -> Result<Self, Error> {
        match token {
            ethabi::Token::FixedBytes(value) => {
                value.as_slice().try_into().map(H256).map_err(|_| {
                    Error::InvalidOutputType(format!("expected bytes32, got {value:?}"))
                })
            }
            _ => Err(Error::InvalidOutputType(format!(
                "expected bytes32, got {token:?}"
            ))),
        }
    }

    fn into_token(self) -> ethabi::Token {
        ethabi::Token::FixedBytes(self.as_bytes().to_vec())
    }
}

impl Tokenizable for Vec<u8> {
    fn from_token(token: ethabi::Token) -> Result<Self, Error> {
        match token {
            ethabi::Token::Bytes(bytes) => Ok(bytes),
            _ => Err(Error::InvalidOutputType(format!(
                "expected bytes, got {token:?}"
            ))),
        }
    }

    fn into_token(self) -> ethabi::Token {
        ethabi::Token::Bytes(self)
    }
}

impl Tokenizable for ethabi::Token {
    fn from_token(token: ethabi::Token) -> Result<Self, Error> {
        Ok(token)
    }

    fn into_token(self) -> ethabi::Token {
        self
    }
}

impl<T: TokenizableItem> Tokenizable for Vec<T> {
    fn from_token(token: ethabi::Token) -> Result<Self, Error> {
        match token {
            ethabi::Token::FixedArray(tokens) | ethabi::Token::Array(tokens) => {
                tokens.into_iter().map(Tokenizable::from_token).collect()
            }
            other => Err(Error::InvalidOutputType(format!(
                "Expected `Array`, got {other:?}"
            ))),
        }
    }

    fn into_token(self) -> ethabi::Token {
        ethabi::Token::Array(self.into_iter().map(Tokenizable::into_token).collect())
    }
}

impl Detokenize for (u32, u32, u32) {
    fn from_tokens(tokens: Vec<Token>) -> anyhow::Result<Self, Error> {
        match tokens.as_slice() {
            [Uint(val1), Uint(val2), Uint(val3)] => {
                Ok((val1.as_u32(), val2.as_u32(), val3.as_u32()))
            }
            other => Err(Error::InvalidOutputType(format!(
                "Expected 3-element `Tuple`, got {other:?}"
            ))),
        }
    }
}

/// Marker trait for `Tokenizable` types that are can tokenized to and from a
/// `Token::Array` and `Token:FixedArray`.
pub trait TokenizableItem: Tokenizable {}

impl TokenizableItem for ethabi::Token {}
impl TokenizableItem for Vec<u8> {}
