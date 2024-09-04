use crate::{ethabi::Token, U256};

// uint256 txType;
// uint256 from;
// uint256 to;
// uint256 gasLimit;
// uint256 gasPerPubdataByteLimit;
// uint256 maxFeePerGas;
// uint256 maxPriorityFeePerGas;
// uint256 paymaster;
// uint256 nonce;
// uint256 value;
// // In the future, we might want to add some
// // new fields to the struct. The `txData` struct
// // is to be passed to account and any changes to its structure
// // would mean a breaking change to these accounts. To prevent this,
// // we should keep some fields as "reserved"
// // It is also recommended that their length is fixed, since
// // it would allow easier proof integration (in case we will need
// // some special circuit for preprocessing transactions)
// uint256[4] reserved;
// bytes data;
// bytes signature;
// uint256[] factoryDeps;
// bytes paymasterInput;
// // Reserved dynamic type for the future use-case. Using it should be avoided,
// // But it is still here, just in case we want to enable some additional functionality
// bytes reservedDynamic;
#[derive(Debug, Clone)]
pub struct L2CanonicalTransaction {
    pub tx_type: U256,
    pub from: U256,
    pub to: U256,
    pub gas_limit: U256,
    pub gas_per_pubdata_byte_limit: U256,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub paymaster: U256,
    pub nonce: U256,
    pub value: U256,
    pub reserved: [U256; 4],
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
    pub factory_deps: Vec<U256>,
    pub paymaster_input: Vec<u8>,
    pub reserved_dynamic: Vec<u8>,
}

impl Default for L2CanonicalTransaction {
    fn default() -> Self {
        L2CanonicalTransaction {
            tx_type: U256::zero(),
            from: U256::zero(),
            to: U256::zero(),
            gas_limit: U256::zero(),
            gas_per_pubdata_byte_limit: U256::zero(),
            max_fee_per_gas: U256::zero(),
            max_priority_fee_per_gas: U256::zero(),
            paymaster: U256::zero(),
            nonce: U256::zero(),
            value: U256::zero(),
            reserved: [U256::zero(); 4],
            data: vec![],
            signature: vec![],
            factory_deps: vec![],
            paymaster_input: vec![],
            reserved_dynamic: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct BridgeHubRequestL2TransactionOnGateway {
    pub chaind_id: U256,
    pub transaction: L2CanonicalTransaction, // Corresponds to L2CanonicalTransaction calldata _transaction
    pub factory_deps: Vec<Vec<u8>>,          // Corresponds to bytes[] calldata _factoryDeps
    pub canonical_tx_hash: [u8; 32],         // Corresponds to bytes32 _canonicalTxHash
    pub expiration_timestamp: u64,           // Corresponds to uint64 _expirationTimestamp
}

impl Default for BridgeHubRequestL2TransactionOnGateway {
    fn default() -> Self {
        BridgeHubRequestL2TransactionOnGateway {
            chaind_id: U256::from(273),
            transaction: L2CanonicalTransaction::default(),
            factory_deps: vec![],
            canonical_tx_hash: [0u8; 32],
            expiration_timestamp: 0,
        }
    }
}

impl BridgeHubRequestL2TransactionOnGateway {
    pub fn to_tokens(&self) -> Vec<Token> {
        let L2CanonicalTransaction {
            tx_type,
            from,
            to,
            gas_limit,
            gas_per_pubdata_byte_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            paymaster,
            nonce,
            value,
            reserved,
            data,
            signature,
            factory_deps,
            paymaster_input,
            reserved_dynamic,
        } = &self.transaction;

        let transaction_tokens = vec![
            Token::Uint(*tx_type),
            Token::Uint(*from),
            Token::Uint(*to),
            Token::Uint(*gas_limit),
            Token::Uint(*gas_per_pubdata_byte_limit),
            Token::Uint(*max_fee_per_gas),
            Token::Uint(*max_priority_fee_per_gas),
            Token::Uint(*paymaster),
            Token::Uint(*nonce),
            Token::Uint(*value),
            Token::FixedArray(reserved.iter().map(|r| Token::Uint(*r)).collect()),
            Token::Bytes(data.clone()),
            Token::Bytes(signature.clone()),
            Token::Array(factory_deps.iter().map(|d| Token::Uint(*d)).collect()),
            Token::Bytes(paymaster_input.clone()),
            Token::Bytes(reserved_dynamic.clone()),
        ];

        vec![
            Token::Uint(self.chaind_id),
            Token::Tuple(transaction_tokens),
            Token::Array(
                self.factory_deps
                    .iter()
                    .map(|dep| Token::Bytes(dep.clone()))
                    .collect(),
            ),
            Token::FixedBytes(self.canonical_tx_hash.to_vec()),
            Token::Uint(U256::from(self.expiration_timestamp)),
        ]
    }
}
