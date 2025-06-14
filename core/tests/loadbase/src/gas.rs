use ethers::{
    prelude::*,
    types::transaction::eip2718::TypedTransaction,
    types::U256,
};

pub enum GasMode {
    Fixed(U256),
    EstimateOnce,
}

pub async fn resolve<P: JsonRpcClient + 'static>(
    provider: &Provider<P>,
    from: Address,
    amount: U256,
    mode: GasMode,
) -> anyhow::Result<U256> {
    match mode {
        GasMode::Fixed(v) => Ok(v),
        GasMode::EstimateOnce => {
            let tx: TypedTransaction = TransactionRequest::pay(from, amount).from(from).into();
            Ok(provider.estimate_gas(&tx, None).await?)
        }
    }
}
