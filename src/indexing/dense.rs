use crate::indexing::shared::{decode_record, Event, RECORD_SIZE};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
#[doc = ""]
pub struct DenseIndexBuilder {
    index_file: File,
}
#[doc = ""]

impl DenseIndexBuilder {
    #[doc = ""]
    pub fn create(index_path: &str) -> std::io::Result<Self> {
        let index_file = File::create(index_path)?;
        Ok(Self { index_file })
    }

    #[doc = ""]
    pub fn add_entry(&mut self, timestamp: u64, offset: u64) -> std::io::Result<()> {
        self.index_file.write_all(&timestamp.to_be_bytes())?;
        self.index_file.write_all(&offset.to_be_bytes())?;
        Ok(())
    }

    #[doc = ""]
    pub fn finalize(&mut self) -> std::io::Result<()> {
        self.index_file.flush()
    }
}


#[doc=""]
pub fn build_dense_index(
    log_path: &str,
    index_path: &str,
) -> std::io::Result<()> {
    let mut log = File::open(log_path)?;
    let mut builder = DenseIndexBuilder::create(index_path)?;

    let mut offset = 0u64;
    let mut record = [0u8; RECORD_SIZE];

    while log.read_exact(&mut record).is_ok(){
        let (timestamp, _, _ , _, _ ) = decode_record(&record);
        builder.add_entry(timestamp, offset)?;
        offset += RECORD_SIZE as u64;
    }

    builder.finalize()?;
    Ok(())
}

#[doc = ""]
pub struct DenseIndexReader {
    index: Vec<(u64, u64)>,
}

impl DenseIndexReader {
    #[doc = ""]
    pub fn open(index_path: &str) -> std::io::Result<Self> {
        let mut index_file = File::open(index_path)?;
        let mut index = Vec::new();
        let mut entry = [0u8; 16];

        while index_file.read_exact(&mut entry).is_ok() {
            let timestamp = u64::from_be_bytes(entry[0..8].try_into().unwrap());
            let offset = u64::from_be_bytes(entry[8..16].try_into().unwrap());
            index.push((timestamp, offset));
        }
        Ok(Self { index })
    }

    #[doc = ""]
    pub fn query(
        &self,
        log_path: &str,
        timestamp_start_bound: u64,
        timestamp_end_bound: u64,
    ) -> std::io::Result<Vec<Event>> {
        if timestamp_start_bound > timestamp_end_bound {
            return Ok(Vec::new());
        }

        if self.index.is_empty() {
            return Ok(Vec::new());
        }

        let position = self
            .index
            .binary_search_by_key(&timestamp_start_bound, |x| x.0)
            .unwrap_or_else(|i| i.saturating_sub(1));

        let mut log_file = File::open(log_path)?;
        log_file.seek(SeekFrom::Start(self.index[position].1))?;

        let mut results = Vec::new();
        let mut record = [0u8; RECORD_SIZE];

        while log_file.read_exact(&mut record).is_ok() {
            let (timestamp, subject, predicate, object, graph) = decode_record(&record);

            if timestamp > timestamp_end_bound {
                break;
            }

            if timestamp >= timestamp_start_bound {
                results.push(Event { timestamp, subject, predicate, object, graph });
            }
        }
        Ok(results)
    }

    #[doc = ""]
    pub fn index_size_bytes(&self) -> usize {
        self.index.len() * 16
    }
}
