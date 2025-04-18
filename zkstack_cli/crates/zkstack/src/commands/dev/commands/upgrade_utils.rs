use ethers::{
    abi::{parse_abi, Address, Token},
    contract::BaseContract,
    utils::hex,
};
use serde::Serialize;
use zksync_contracts::{chain_admin_contract, hyperchain_contract, DIAMOND_CUT};
use zksync_types::{ethabi, web3::Bytes, U256};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminCall {
    pub(crate) description: String,
    pub(crate) target: Address,
    #[serde(serialize_with = "serialize_hex")]
    pub(crate) data: Vec<u8>,
    pub(crate) value: U256,
}

impl AdminCall {
    fn into_token(self) -> Token {
        let Self {
            target,
            data,
            value,
            ..
        } = self;
        Token::Tuple(vec![
            Token::Address(target),
            Token::Uint(value),
            Token::Bytes(data),
        ])
    }
}

fn serialize_hex<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let hex_string = format!("0x{}", hex::encode(bytes));
    serializer.serialize_str(&hex_string)
}

#[derive(Debug, Clone)]
pub struct AdminCallBuilder {
    calls: Vec<AdminCall>,
    validator_timelock_abi: BaseContract,
    zkchain_abi: ethabi::Contract,
    chain_admin_abi: ethabi::Contract,
}

impl AdminCallBuilder {
    pub fn new() -> Self {
        Self {
            calls: vec![],
            validator_timelock_abi: BaseContract::from(
                parse_abi(&[
                    "function addValidator(uint256 _chainId, address _newValidator) external",
                ])
                .unwrap(),
            ),
            zkchain_abi: hyperchain_contract(),
            chain_admin_abi: chain_admin_contract(),
        }
    }

    pub fn append_validator(
        &mut self,
        chain_id: u64,
        validator_timelock_addr: Address,
        validator_addr: Address,
    ) {
        let data = self
            .validator_timelock_abi
            .encode("addValidator", (U256::from(chain_id), validator_addr))
            .unwrap();
        let description = format!(
            "Adding validator 0x{}",
            hex::encode(validator_timelock_addr.0)
        );

        let call = AdminCall {
            description,
            data: data.to_vec(),
            target: validator_timelock_addr,
            value: U256::zero(),
        };

        self.calls.push(call);
    }

    pub fn append_execute_upgrade(
        &mut self,
        hyperchain_addr: Address,
        protocol_version: u64,
        diamond_cut_data: Bytes,
    ) {
        let diamond_cut = DIAMOND_CUT.decode_input(&diamond_cut_data.0).unwrap()[0].clone();

        let data = self
            .zkchain_abi
            .function("upgradeChainFromVersion")
            .unwrap()
            .encode_input(&[Token::Uint(protocol_version.into()), diamond_cut])
            .unwrap();
        let description = "Executing upgrade:".to_string();

        let call = AdminCall {
            description,
            data: data.to_vec(),
            target: hyperchain_addr,
            value: U256::zero(),
        };

        self.calls.push(call);
    }

    pub fn append_set_da_validator_pair(
        &mut self,
        hyperchain_addr: Address,
        l1_da_validator: Address,
        l2_da_validator: Address,
    ) {
        let data = self
            .zkchain_abi
            .function("setDAValidatorPair")
            .unwrap()
            .encode_input(&[
                Token::Address(l1_da_validator),
                Token::Address(l2_da_validator),
            ])
            .unwrap();
        let description = "Setting DA validator pair".to_string();

        let call = AdminCall {
            description,
            data: data.to_vec(),
            target: hyperchain_addr,
            value: U256::zero(),
        };

        self.calls.push(call);
    }

    pub fn append_make_permanent_rollup(&mut self, hyperchain_addr: Address) {
        let data = self
            .zkchain_abi
            .function("makePermanentRollup")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        let description = "Make permanent rollup:".to_string();

        let call = AdminCall {
            description,
            data: data.to_vec(),
            target: hyperchain_addr,
            value: U256::zero(),
        };

        self.calls.push(call);
    }

    pub fn display(&self) {
        // Serialize with pretty printing
        let serialized = serde_json::to_string_pretty(&self.calls).unwrap();

        // Output the serialized JSON
        println!("{}", serialized);
    }

    pub fn compile_full_calldata(self) -> Vec<u8> {
        let tokens: Vec<_> = self.calls.into_iter().map(|x| x.into_token()).collect();

        let data = self
            .chain_admin_abi
            .function("multicall")
            .unwrap()
            .encode_input(&[Token::Array(tokens), Token::Bool(true)])
            .unwrap();

        data.to_vec()
    }
}

pub(crate) fn print_error(err: anyhow::Error) {
    println!(
        "Chain is not ready to finalize the upgrade due to the reason:\n{:#?}",
        err
    );
    println!("Once the chain is ready, you can re-run this command to obtain the calls to finalize the upgrade");
    println!("If you want to display finalization params anyway, pass `--force-display-finalization-params=true`.");
}

fn chain_admin_abi() -> BaseContract {
    BaseContract::from(
        parse_abi(&[
            "function setUpgradeTimestamp(uint256 _protocolVersion, uint256 _upgradeTimestamp) external",
        ])
        .unwrap(),
    )
}

pub(crate) fn set_upgrade_timestamp_calldata(
    packed_protocol_version: u64,
    timestamp: u64,
) -> Vec<u8> {
    let chain_admin = chain_admin_abi();

    chain_admin
        .encode("setUpgradeTimestamp", (packed_protocol_version, timestamp))
        .unwrap()
        .to_vec()
}
