use std::mem;

use crate::{
    ethabi::Token,
    web3::contract::{tokens::Tokenizable, Error},
    Address,
};

/// Multicall3 contract aggregate method input vector struct.
pub struct Multicall3Call {
    pub target: Address,
    pub allow_failure: bool,
    pub calldata: Vec<u8>,
}

impl Tokenizable for Multicall3Call {
    fn into_token(self) -> Token {
        Token::Tuple(vec![
            self.target.into_token(),
            self.allow_failure.into_token(),
            self.calldata.into_token(),
        ])
    }
    fn from_token(token: Token) -> Result<Self, Error> {
        let Token::Tuple(mut result_token) = token else {
            return Err(error(&[token], "Multicall3Call"));
        };
        let [Token::Address(target), Token::Bool(allow_failure), Token::Bytes(calldata)] =
            result_token.as_mut_slice()
        else {
            return Err(error(&result_token, "Multicall3Call"));
        };

        Ok(Multicall3Call {
            target: *target,
            allow_failure: *allow_failure,
            calldata: mem::take(calldata),
        })
    }
}

/// Multicall3 contract call's output vector struct.
pub struct Multicall3Result {
    pub success: bool,
    pub return_data: Vec<u8>,
}

impl Tokenizable for Multicall3Result {
    fn from_token(token: Token) -> Result<Multicall3Result, Error> {
        let Token::Tuple(mut result_token) = token else {
            return Err(error(&[token], "Multicall3Result"));
        };
        let [Token::Bool(success), Token::Bytes(return_data)] = result_token.as_mut_slice() else {
            return Err(error(&result_token, "Multicall3Result"));
        };

        Ok(Multicall3Result {
            success: *success,
            return_data: mem::take(return_data),
        })
    }

    fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::Bool(self.success),
            Token::Bytes(self.return_data),
        ])
    }
}

fn error(token: &[Token], result_struct_name: &str) -> Error {
    Error::InvalidOutputType(format!(
        "Expected `{result_struct_name}` token, got token: {token:?}"
    ))
}
