use num::Zero;

const L2_TO_L1_LOG_LEN: usize = 1 + 1 + 2 + 20 + 32 + 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PubdataMetrics {
    pub l2_to_l1_logs: u32,
    pub l1_to_l2_messages: u32,
    pub bytecodes: u32,
    pub bytes_per_enumeration_index: u8,
    pub initial_writes: u16,
    pub repeated_writes: u16,
}

impl PubdataMetrics {
    pub fn parse(bytes: &[u8]) -> Self {
        PubdataMetricsParser::new(bytes).parse()
    }
}

struct PubdataMetricsParser<'a> {
    bytes: &'a [u8],
    ptr: &'a [u8],
}

impl<'a> PubdataMetricsParser<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, ptr: bytes }
    }

    fn offset(&self) -> isize {
        unsafe { self.ptr.as_ptr().byte_offset_from(self.bytes.as_ptr()) }
    }

    fn die(&self, msg: &str) -> ! {
        panic!("[{}]: {msg}", self.offset())
    }

    fn skip_compressed_value(&mut self) {
        let metadata_byte = self.take::<1>()[0];
        let operation_id = metadata_byte & 0b111;
        let output_size = if operation_id == 0 {
            32
        } else {
            metadata_byte >> 3
        };

        if operation_id > 3 {
            self.die(&format!("Invalid operation ID {operation_id}"))
        }

        self.skip(output_size as usize);
    }

    fn skip_initial_write(&mut self) {
        // key is uncompressed in initial writes
        self.skip(32);
        self.skip_compressed_value();
    }

    fn skip_repeated_write(&mut self) {
        // enumeration index
        self.skip(4);
        self.skip_compressed_value();
    }

    fn take<const N: usize>(&mut self) -> [u8; N] {
        let mut buf = [0u8; N];
        if self.ptr.len() < N {
            self.die(&format!(
                "Overread. Requested {N}, available {}",
                self.ptr.len(),
            ))
        }

        buf.copy_from_slice(&self.ptr[0..N]);
        self.ptr = &self.ptr[N..];
        buf
    }

    fn skip(&mut self, n: usize) {
        self.ptr = &self.ptr[n..];
    }

    pub fn parse(&mut self) -> PubdataMetrics {
        let l2_to_l1_logs = u32::from_be_bytes(self.take());
        self.skip(l2_to_l1_logs as usize * L2_TO_L1_LOG_LEN);

        let l1_to_l2_messages = u32::from_be_bytes(self.take());
        for _ in 0..l1_to_l2_messages {
            let len = u32::from_be_bytes(self.take());
            self.skip(len as usize);
        }

        let bytecodes = u32::from_be_bytes(self.take());
        for _ in 0..bytecodes {
            let len = u32::from_be_bytes(self.take());
            self.skip(len as usize);
        }

        let compression_version = u8::from_be_bytes(self.take());
        if compression_version != 1 {
            self.die(&format!(
                "Unknown compression version {compression_version}",
            ));
        }
        let _compressed_length = {
            let mut buf = [0u8; 4];
            buf[1..].copy_from_slice(&self.take::<3>());
            u32::from_be_bytes(buf)
        };
        let bytes_per_enumeration_index = u8::from_be_bytes(self.take());

        // skip initial writes
        let initial_writes = u16::from_be_bytes(self.take());
        for _ in 0..initial_writes {
            self.skip_initial_write();
        }

        // skip and count repeated writes
        let mut repeated_writes: u16 = 0;
        while !self.ptr.len().is_zero() {
            self.skip_repeated_write();
            repeated_writes += 1;
        }

        PubdataMetrics {
            l2_to_l1_logs,
            l1_to_l2_messages,
            bytecodes,
            bytes_per_enumeration_index,
            initial_writes,
            repeated_writes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic = "Unknown compression version 0"]
    fn test_failure_0() {
        PubdataMetrics::parse(&[0u8; 1024]);
    }

    #[test]
    #[should_panic = "out of range"]
    fn test_failure_1() {
        PubdataMetrics::parse(&[1u8; 1024]);
    }

    macro_rules! make_test {
        ($name:ident,$path:expr) => {
            #[test]
            fn $name() {
                static BIN: &[u8] = include_bytes!($path);
                dbg!(PubdataMetricsParser::new(BIN).parse());
            }
        };
    }

    make_test!(test_parse_0, "../dump/pubdata_0");
    make_test!(test_parse_1, "../dump/pubdata_1");
    make_test!(test_parse_2, "../dump/pubdata_2");
    make_test!(test_parse_3, "../dump/pubdata_3");
    make_test!(test_parse_4, "../dump/pubdata_4");
    make_test!(test_parse_5, "../dump/pubdata_5");
}
