use crate::indexing::shared::{decode_record, Event, RECORD_SIZE};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

#[doc=""]
pub struct SparseIndexBuilder {
    index_file: File,
    interval: usize,
}
#[doc=""]
impl SparseIndexBuilder {
    
    #[doc=""]
    pub fn create(
        index_path: &str,
        interval: usize,
    ) -> std::io::Result<Self> {
        let index_file = File::create(index_path)?;
        Ok (Self {
            index_file,
            interval,
        })
    }
    
    #[doc=""]
    pub fn add_entry(
        &mut self, 
        record_count: u64,
        timestamp: u64,
        offset: u64,
    ) -> std::io::Result<bool> {
        if record_count % self.interval as u64 == 0 {
            self.index_file.write_all(&timestamp.to_be_bytes())?;
            self.index_file.write_all(&offset.to_be_bytes())?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    #[doc=""]
    pub fn finalize(&mut self) -> std::io::Result<()> {
        self.index_file.flush()
    }
}

// pub fn build_sparse_index(
//     log_path: &str,
// ) 