use crate::glue::GlueFrom;
use crate::vm_latest::VmMemoryMetrics;

impl GlueFrom<crate::vm_virtual_blocks::VmMemoryMetrics> for VmMemoryMetrics {
    fn glue_from(value: crate::vm_virtual_blocks::VmMemoryMetrics) -> Self {
        Self {
            event_sink_inner: value.event_sink_inner,
            event_sink_history: value.event_sink_history,
            memory_inner: value.memory_inner,
            memory_history: value.memory_history,
            decommittment_processor_inner: value.decommittment_processor_inner,
            decommittment_processor_history: value.decommittment_processor_history,
            storage_inner: value.storage_inner,
            storage_history: value.storage_history,
        }
    }
}
