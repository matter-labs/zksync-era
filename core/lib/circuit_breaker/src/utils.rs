use zksync_types::ethabi::Token;

pub fn unwrap_tuple(token: Token) -> Vec<Token> {
    if let Token::Tuple(tokens) = token {
        tokens
    } else {
        panic!("Tuple was expected, got: {}", token);
    }
}
