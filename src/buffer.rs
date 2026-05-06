use std::path::Path;

use serde_yaml;
use sled::IVec;
use thiserror::Error;

use crate::types::TagSample;

#[derive(Debug, Error)]
pub enum BufferError {
    #[error("sled buffer error: {0}")]
    Sled(#[from] sled::Error),
    #[error("buffer serialization error: {0}")]
    Serde(#[from] serde_yaml::Error),
    #[error("buffer size limit exceeded: {current_bytes} bytes >= {max_bytes} bytes")]
    SizeLimit {
        current_bytes: u64,
        max_bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BufferedSample {
    pub key: Vec<u8>,
    pub sample: TagSample,
}

#[derive(Clone)]
pub struct SampleBuffer {
    db: sled::Db,
    max_size_bytes: u64,
}

impl SampleBuffer {
    pub fn open(path: impl AsRef<Path>, max_size_mb: u64) -> Result<Self, BufferError> {
        let db = sled::open(path)?;
        Ok(Self {
            db,
            max_size_bytes: max_size_mb.saturating_mul(1024 * 1024),
        })
    }

    pub fn push_many(&self, samples: &[TagSample]) -> Result<usize, BufferError> {
        if samples.is_empty() {
            return Ok(0);
        }
        self.ensure_size_available()?;

        for sample in samples {
            let key = self.db.generate_id()?.to_be_bytes();
            let payload = serde_yaml::to_string(sample)?.into_bytes();
            self.db.insert(key, payload)?;
        }

        Ok(samples.len())
    }

    pub fn drain_batch(&self, limit: usize) -> Result<Vec<BufferedSample>, BufferError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut entries = Vec::with_capacity(limit);
        for item in self.db.iter().take(limit) {
            let (key, value) = item?;
            entries.push(BufferedSample {
                key: key.to_vec(),
                sample: deserialize_sample(&value)?,
            });
        }
        Ok(entries)
    }

    pub fn ack(&self, entries: &[BufferedSample]) -> Result<usize, BufferError> {
        for entry in entries {
            self.db.remove(&entry.key)?;
        }
        Ok(entries.len())
    }

    pub fn len(&self) -> usize {
        self.db.len()
    }

    pub fn is_empty(&self) -> bool {
        self.db.is_empty()
    }

    pub fn flush(&self) -> Result<(), BufferError> {
        self.db.flush()?;
        Ok(())
    }

    fn ensure_size_available(&self) -> Result<(), BufferError> {
        let current_bytes = self.db.size_on_disk()?;
        if current_bytes >= self.max_size_bytes {
            return Err(BufferError::SizeLimit {
                current_bytes,
                max_bytes: self.max_size_bytes,
            });
        }
        Ok(())
    }
}

fn deserialize_sample(value: &IVec) -> Result<TagSample, serde_yaml::Error> {
    serde_yaml::from_slice(value.as_ref())
}

