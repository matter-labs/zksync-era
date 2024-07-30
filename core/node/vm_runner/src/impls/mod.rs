//! Components powered by a VM runner.

mod bwip;
mod playground;
mod protective_reads;

pub use self::{
    bwip::{
        BasicWitnessInputProducer, BasicWitnessInputProducerIo, BasicWitnessInputProducerTasks,
    },
    playground::{VmPlayground, VmPlaygroundIo, VmPlaygroundTasks},
    protective_reads::{ProtectiveReadsIo, ProtectiveReadsWriter, ProtectiveReadsWriterTasks},
};
