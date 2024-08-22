use anyhow::Context;
use zksync_concurrency::{ctx, error::Wrap as _};
use zksync_contracts::consensus_l2_contracts as contracts;
use zksync_consensus_roles::{validator,attester};
use zksync_consensus_crypto::ByteFmt;
use zksync_node_api_server::{
    execution_sandbox::{VmConcurrencyLimiter,TxSharedArgs},
    tx_sender::{MultiVMBaseSystemContracts}, tx_sender::TxSender};
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_node_api_server::execution_sandbox::TransactionExecutor;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    AccountTreeId,
    ethabi,
    fee_model::BatchFeeInput,
    fee::Fee,
    l2::L2Tx,
    transaction_request::CallOverrides,
    Nonce, U256,
};
use crate::storage::{ConnectionPool,Connection};

#[cfg(test)]
mod tests;

/// A struct for reading data from consensus L2 contracts.
#[derive(Debug)]
pub(crate) struct VM {
    contract: contracts::ConsensusRegistry,
    address: ethabi::Address,
    pool: ConnectionPool,
    tx_shared_args: TxSharedArgs,
    limiter: VmConcurrencyLimiter,
}

pub(crate) struct AddInputs {
    node_owner: ethabi::Address,
    validator: WeightedValidator,
    attester: attester::WeightedAttester,
}

pub(crate) struct WeightedValidator {
    weight: validator::Weight,
    key: validator::PublicKey,
    pop: validator::ProofOfPossession,
}

impl AddInputs {
    fn encode(&self) -> anyhow::Result<contracts::AddInputs> {
        Ok(contracts::AddInputs {
            node_owner: self.node_owner,
            validator_pub_key: encode_validator_key(&self.validator.key),
            validator_weight: self.validator.weight.into(),
            validator_pop: encode_validator_pop(&self.validator.pop),
            attester_pub_key: encode_attester_key(&self.attester.key),
            attester_weight: self.attester.weight.try_into().context("overflow")?,
        })
    }
}

fn encode_attester_key(k : &attester::PublicKey) -> contracts::Secp256k1PublicKey {
    let b: [u8;33] = ByteFmt::encode(&k).try_into().unwrap();
    Ok(contracts::Secp256k1PublicKey {
        tag: b[0],
        x: b[1..33],
    })
}

fn decode_attester_key(k: &contracts::Secp256k1PublicKey) -> anyhow::Result<attester::PublicKey> {
    let mut x = vec![k.tag];
    x.extend(k.x);
    ByteFmt::decode(&x) 
}

fn encode_validator_key(k: &validator::PublicKey) -> contracts::BLS12_381PublicKey {
    let b: [u8;96] = ByteFmt::encode(k).try_into().unwrap();
    contracts::BLS12_381PublicKey {
        a: b[0..32],
        b: b[32..64],
        c: b[64..96],
    }
}

fn encode_validator_pop(pop: &validator::ProofOfPossession) -> contracts::BLS12_381Signature {
    let b: [u8;48] = ByteFmt::encode(pop).try_into().unwrap();
    contracts::BLS12_381Signature {
        a: b[0..32],
        b: b[32..48],
    }
}

/*
fn decode_validator_key(k: contracts::BLS12_381PublicKey) -> anyhow::Result<validator::PublicKey> {
    let mut x = Vec::from(k.a);
    x.extend(k.b);
    x.extend(k.c);
    ByteFmt::decode(&x)
}

fn encode_weighted_attester(a: attester::WeightedAttester) -> anyhow::Result<contracts::Attester> {
    Ok(contracts::Attester {
        weight: a.weight.try_into().context("overflow")?,
        pub_key: encode_validator_key(&a.key),
    })
}*/

fn decode_weighted_attester(a: &contracts::Attester) -> anyhow::Result<attester::WeightedAttester> {
    Ok(attester::WeightedAttester {
        weight: a.weight.into(),
        key: decode_attester_key(&a.pub_key).context("key")?,
    })
}

pub type Calldata = Vec<u8>;

/*
struct VoidTxSink;

impl TxSink for VoidTxSink {
    async fn submit_tx(
        &self,
        _tx: &L2Tx,
        _execution_metrics: TransactionExecutionMetrics,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        unreachable!()
    }
}*/

struct Contract {
    address: ethabi::Address,
    contract: contracts::ConsensusRegistry::load(),
}

impl Contract {
    /// Reads attester committee from the registry contract.
    /// It's implemented by dispatching multiple read transactions (a.k.a. `eth_call` requests),
    /// each one carries an instantiation of a separate VM execution sandbox.
    pub async fn get_attester_committee(&self, ctx: &ctx::Ctx, vm: &VM, batch: attester::BatchNumber) -> ctx::Result<attester::Committee> {
        let raw = vm.call(ctx, batch, self.contract.get_attester_commitee(), ())?;
        let mut attesters = vec![];
        for a in raw {
           attesters.push(decode_weighted_attester(&a).context("decode_weighted_attester()")?);
        }
        Ok(attester::Committee::new(attesters.into_iter()).context("Committee::new()")?)
    }
}

impl VM {
    /// Constructs a new `VMReader` instance.
    pub async fn new(pool: ConnectionPool) -> anyhow::Result<Self> {
        Self {
            pool,
            tx_shared_args: TxSharedArgs {
                operator_account: AccountTreeId::default(),
                fee_input: BatchFeeInput::sensible_l1_pegged_default(),
                base_system_contracts: scope::wait_blocking(MultiVMBaseSystemContracts::load_eth_call_blocking),
                caches: PostgresStorageCaches::new(1, 1),
                validation_computational_gas_limit: u32::MAX,
                chain_id: L2ChainId::default(),
                whitelisted_tokens_for_aa: vec![],
            },
            limiter: VmConcurrencyLimiter::new(1).0,
        }
    }

    async fn call<Sig: contracts::FunctionSig>(&self, ctx: &ctx::Ctx, batch: attester::BatchNumber, f: contracts::Function<'_, Sig>, input: Sig::Inputs) -> ctx::Result<Sig::Outputs> {
        let tx = L2Tx::new(
            self.address,
            f.encode_input(input).context("encode_input")?,
            Nonce(0),
            Fee {
                gas_limit: U256::from(2000000000u32),
                max_fee_per_gas: U256::zero(),
                max_priority_fee_per_gas: U256::zero(),
                gas_per_pubdata_limit: U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE),
            },
            ethabi::Address::zero(),
            U256::zero(),
            vec![],
            Default::default(),
        );
        let args = self.pool.connection().await.wrap("connection()")?.block_args(ctx,batch).await.wrap("block_args()")?;
        let permit = ctx.wait(self.limiter.acquire()).await?.unwrap();
        let output = ctx.wait(TransactionExecutor::Real.execute_tx_eth_call(
            permit,
            self.tx_shared_args.clone(), 
            self.pool.clone(),
            CallOverrides { enforced_base_fee: None },
            tx,
            args,
            None,
            vec![],
            None,
        )).await?.context("execute_tx_eth_call()")?;
        match output.result {
            ExecutionResult::Success { output } => Ok(f.decode_output(&output).context("decode_output()")?),
            other => Err(anyhow::format_err!("unsuccessful execution: {other:?").into()),
        }
    }
}
