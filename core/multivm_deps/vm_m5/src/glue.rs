pub trait GlueFrom<T>: Sized {
    fn glue_from(value: T) -> Self;
}

/// See the description of [`GlueFrom`] trait above.
pub trait GlueInto<T>: Sized {
    fn glue_into(self) -> T;
}

// Blaknet `GlueInto` impl for any type that implements `GlueFrom`.
impl<T, U> GlueInto<U> for T
where
    U: GlueFrom<T>,
{
    fn glue_into(self) -> U {
        U::glue_from(self)
    }
}

// Identity impl.
impl<T> GlueFrom<T> for T {
    fn glue_from(this: T) -> Self {
        this
    }
}

impl GlueFrom<zk_evm::aux_structures::Timestamp>
    for zksync_types::zk_evm::aux_structures::Timestamp
{
    fn glue_from(timestamp: zk_evm::aux_structures::Timestamp) -> Self {
        zksync_types::zk_evm::aux_structures::Timestamp(timestamp.0)
    }
}

impl GlueFrom<zk_evm::aux_structures::LogQuery> for zksync_types::zk_evm::aux_structures::LogQuery {
    fn glue_from(query: zk_evm::aux_structures::LogQuery) -> Self {
        zksync_types::zk_evm::aux_structures::LogQuery {
            address: query.address,
            key: query.key,
            written_value: query.written_value,
            timestamp: query.timestamp.glue_into(),
            shard_id: query.shard_id,
            rollback: query.rollback,
            tx_number_in_block: query.tx_number_in_block,
            aux_byte: query.aux_byte,
            read_value: query.read_value,
            rw_flag: query.rw_flag,
            is_service: query.is_service,
        }
    }
}

impl GlueFrom<zksync_types::zk_evm::aux_structures::Timestamp>
    for zk_evm::aux_structures::Timestamp
{
    fn glue_from(timestamp: zksync_types::zk_evm::aux_structures::Timestamp) -> Self {
        zk_evm::aux_structures::Timestamp(timestamp.0)
    }
}

impl GlueFrom<zksync_types::zk_evm::aux_structures::LogQuery> for zk_evm::aux_structures::LogQuery {
    fn glue_from(query: zksync_types::zk_evm::aux_structures::LogQuery) -> Self {
        zk_evm::aux_structures::LogQuery {
            address: query.address,
            key: query.key,
            written_value: query.written_value,
            timestamp: query.timestamp.glue_into(),
            shard_id: query.shard_id,
            rollback: query.rollback,
            tx_number_in_block: query.tx_number_in_block,
            aux_byte: query.aux_byte,
            read_value: query.read_value,
            rw_flag: query.rw_flag,
            is_service: query.is_service,
        }
    }
}

impl GlueFrom<zk_evm::reference_impls::event_sink::EventMessage>
    for zksync_types::zk_evm::reference_impls::event_sink::EventMessage
{
    fn glue_from(event: zk_evm::reference_impls::event_sink::EventMessage) -> Self {
        zksync_types::zk_evm::reference_impls::event_sink::EventMessage {
            shard_id: event.shard_id,
            is_first: event.is_first,
            tx_number_in_block: event.tx_number_in_block,
            address: event.address,
            key: event.key,
            value: event.value,
        }
    }
}
