use std::error::Error;

pub type BitcoinClientResult<T> = Result<T, Box<dyn Error>>;
pub type BitcoinSignerResult<T> = Result<T, Box<dyn Error>>;
pub type BitcoinInscriberResult<T> = Result<T, Box<dyn Error>>;
pub type BitcoinInscriptionIndexerResult<T> = Result<T, Box<dyn Error>>;
pub type BitcoinTransactionBuilderResult<T> = Result<T, Box<dyn Error>>;
