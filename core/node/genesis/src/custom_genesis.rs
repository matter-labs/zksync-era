use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    marker::PhantomData,
    os::unix::fs::FileExt,
};

use zksync_types::{AccountTreeId, Address, StorageKey, StorageLog, H160, H256};

#[derive(Debug, Clone)]
pub struct StorageLogExport {
    pub address: Address,
    pub key: H256,
    pub value: H256,
}

impl From<StorageLogExport> for StorageLog {
    fn from(value: StorageLogExport) -> Self {
        StorageLog::new_write_log(
            StorageKey::new(AccountTreeId::new(value.address), value.key),
            value.value,
        )
    }
}

#[derive(Debug, Clone)]
pub struct FactoryDepExport {
    pub bytecode_hash: H256,
    pub bytecode: Vec<u8>,
}

#[derive(Debug)]
pub struct GenesisExportReader {
    file: File,
    storage_logs_offset: usize,
    storage_logs_count: usize,
    factory_deps_offset: usize,
    factory_deps_count: usize,
}

fn randreadn<const N: usize>(file: &File, at: usize) -> [u8; N] {
    let mut buf = [0u8; N];
    file.read_exact_at(&mut buf, at as u64).unwrap();
    buf
}

impl GenesisExportReader {
    const STORAGE_LOG_EXPORT_SIZE: usize = 20 + 32 + 32;

    pub fn new(file: File) -> Self {
        let storage_logs_count = u32::from_le_bytes(randreadn(&file, 0)) as usize;
        let storage_logs_offset = 4;
        let factory_deps_count_offset =
            storage_logs_offset + Self::STORAGE_LOG_EXPORT_SIZE * storage_logs_count;
        let factory_deps_count =
            u32::from_le_bytes(randreadn(&file, factory_deps_count_offset)) as usize;
        let factory_deps_offset = factory_deps_count_offset + 4;

        tracing::trace!("Genesis export reader: {storage_logs_count}, {factory_deps_count}");

        Self {
            file,
            storage_logs_count,
            storage_logs_offset,
            factory_deps_count,
            factory_deps_offset,
        }
    }

    pub fn storage_logs(&self) -> ExportItemReader<StorageLogExport> {
        let mut buf_reader = BufReader::new(&self.file);
        buf_reader
            .seek(SeekFrom::Start(self.storage_logs_offset as u64))
            .unwrap();

        ExportItemReader::new(buf_reader, self.storage_logs_count)
    }

    pub fn factory_deps(&self) -> ExportItemReader<FactoryDepExport> {
        let mut buf_reader = BufReader::new(&self.file);
        buf_reader
            .seek(SeekFrom::Start(self.factory_deps_offset as u64))
            .unwrap();

        ExportItemReader::new(buf_reader, self.factory_deps_count)
    }
}

fn readn<const N: usize>(reader: &mut impl BufRead) -> [u8; N] {
    let mut buf = [0u8; N];
    reader.read_exact(&mut buf).unwrap();
    buf
}

trait ExportItem {
    fn read(reader: &mut impl BufRead) -> Self;
}

#[derive(Debug)]
pub struct ExportItemReader<'a, T> {
    _item: PhantomData<T>,
    reader: BufReader<&'a File>,
    count: usize,
    next_index: usize,
}

impl<'a, T> ExportItemReader<'a, T> {
    pub fn new(reader: BufReader<&'a File>, count: usize) -> Self {
        Self {
            _item: PhantomData,
            next_index: 0,
            count,
            reader,
        }
    }
}

impl<'a, T: ExportItem> Iterator for ExportItemReader<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.count {
            return None;
        }

        self.next_index += 1;

        Some(Self::Item::read(&mut self.reader))
    }
}

impl ExportItem for StorageLogExport {
    fn read(reader: &mut impl BufRead) -> Self {
        let address = H160(readn(reader));
        let key = H256(readn(reader));
        let value = H256(readn(reader));

        Self {
            address,
            key,
            value,
        }
    }
}

impl ExportItem for FactoryDepExport {
    fn read(reader: &mut impl BufRead) -> Self {
        let bytecode_hash = H256(readn(reader));
        let bytecode_len = u64::from_le_bytes(readn(reader)) as usize;
        let mut bytecode = vec![0u8; bytecode_len];
        reader.read_exact(&mut bytecode).unwrap();

        Self {
            bytecode_hash,
            bytecode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_reader() {
        let path = "tests/data/genesis_export.bin";
        let reader = GenesisExportReader::new(File::open(path).unwrap());

        let mut count_sl = 0;
        for _ in reader.storage_logs() {
            count_sl += 1;
        }
        assert_eq!(count_sl, 8804);

        let mut count_fd = 0;
        for _ in reader.factory_deps() {
            count_fd += 1;
        }
        assert_eq!(count_fd, 57);
    }
}