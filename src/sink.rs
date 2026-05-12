use std::mem;
use std::time::Duration;

use sqlx::{mysql::MySqlPoolOptions, MySqlPool};
use thiserror::Error;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

use crate::buffer::{BufferedSample, SampleBuffer};
use crate::config::MysqlConfig;
use crate::types::TagSample;

#[derive(Debug, Error)]
pub enum SinkError {
    #[error("invalid MySQL identifier {0:?}")]
    InvalidIdentifier(String),
    #[error("cannot build INSERT for zero rows")]
    EmptyInsert,
    #[error("mysql error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("buffer error: {0}")]
    Buffer(#[from] crate::buffer::BufferError),
}

pub fn validate_mysql_identifier(identifier: &str) -> Result<(), SinkError> {
    let mut chars = identifier.chars();
    let Some(first) = chars.next() else {
        return Err(SinkError::InvalidIdentifier(identifier.to_string()));
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(SinkError::InvalidIdentifier(identifier.to_string()));
    }
    if !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        return Err(SinkError::InvalidIdentifier(identifier.to_string()));
    }
    Ok(())
}

pub fn build_insert_sql(table: &str, rows: usize) -> Result<String, SinkError> {
    validate_mysql_identifier(table)?;
    if rows == 0 {
        return Err(SinkError::EmptyInsert);
    }

    let placeholders = std::iter::repeat("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .take(rows)
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!(
        "INSERT INTO `{table}` \
         (`node_id`, `alias`, `area`, `device`, `value_type`, `value_num`, `value_str`, `source_ts`, `server_ts`, `quality`, `source`) \
         VALUES {placeholders}"
    ))
}

pub async fn connect_mysql(config: &MysqlConfig) -> Result<MySqlPool, sqlx::Error> {
    MySqlPoolOptions::new()
        .max_connections(config.max_connections)
        .connect(&config.url)
        .await
}

pub async fn insert_samples(
    pool: &MySqlPool,
    table: &str,
    samples: &[TagSample],
) -> Result<u64, SinkError> {
    if samples.is_empty() {
        return Ok(0);
    }

    let sql = build_insert_sql(table, samples.len())?;
    let mut query = sqlx::query(&sql);
    for sample in samples {
        query = query
            .bind(&sample.node_id)
            .bind(&sample.alias)
            .bind(&sample.area)
            .bind(&sample.device)
            .bind(sample.value_type_code())
            .bind(sample.value_num())
            .bind(sample.value_str())
            .bind(sample.source_ts.naive_utc())
            .bind(sample.server_ts.naive_utc())
            .bind(sample.quality as i64)
            .bind(&sample.source);
    }

    let result = query.execute(pool).await?;
    Ok(result.rows_affected())
}

#[derive(Debug)]
pub struct BatchBuilder {
    batch_size: usize,
    pending: Vec<TagSample>,
}

impl BatchBuilder {
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch_size,
            pending: Vec::with_capacity(batch_size),
        }
    }

    pub fn push(&mut self, sample: TagSample) -> Option<Vec<TagSample>> {
        self.pending.push(sample);
        if self.pending.len() >= self.batch_size {
            Some(mem::take(&mut self.pending))
        } else {
            None
        }
    }

    pub fn flush(&mut self) -> Vec<TagSample> {
        mem::take(&mut self.pending)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

pub struct SinkWorker {
    pool: MySqlPool,
    table: String,
    receiver: mpsc::Receiver<TagSample>,
    shutdown: watch::Receiver<bool>,
    buffer: SampleBuffer,
    batch_size: usize,
    flush_interval: Duration,
}

impl SinkWorker {
    pub fn new(
        pool: MySqlPool,
        table: String,
        receiver: mpsc::Receiver<TagSample>,
        shutdown: watch::Receiver<bool>,
        buffer: SampleBuffer,
        batch_size: usize,
        flush_interval: Duration,
    ) -> Self {
        Self {
            pool,
            table,
            receiver,
            shutdown,
            buffer,
            batch_size,
            flush_interval,
        }
    }

    pub async fn run(mut self) -> Result<(), SinkError> {
        info!(
            table = %self.table,
            batch_size = self.batch_size,
            flush_interval_ms = self.flush_interval.as_millis(),
            "sink worker started"
        );

        let mut batcher = BatchBuilder::new(self.batch_size);
        let mut interval = tokio::time::interval(self.flush_interval);
        self.replay_buffered().await?;

        loop {
            tokio::select! {
                maybe_sample = self.receiver.recv() => {
                    match maybe_sample {
                        Some(sample) => {
                            metrics::counter!("samples_received_total").increment(1);
                            if let Some(full_batch) = batcher.push(sample) {
                                self.flush_new_samples(&full_batch).await?;
                                self.replay_buffered().await?;
                            }
                        }
                        None => break,
                    }
                }
                _ = interval.tick() => {
                    let batch = batcher.flush();
                    self.flush_new_samples(&batch).await?;
                    self.replay_buffered().await?;
                }
                changed = self.shutdown.changed() => {
                    if changed.is_ok() && *self.shutdown.borrow() {
                        info!("sink worker received shutdown signal");
                        break;
                    }
                }
            }
        }

        while let Ok(sample) = self.receiver.try_recv() {
            if let Some(full_batch) = batcher.push(sample) {
                self.flush_new_samples(&full_batch).await?;
            }
        }

        let final_batch = batcher.flush();
        self.flush_new_samples(&final_batch).await?;
        self.buffer.flush()?;
        info!("sink worker stopped");
        Ok(())
    }

    async fn flush_new_samples(&self, samples: &[TagSample]) -> Result<(), SinkError> {
        if samples.is_empty() {
            return Ok(());
        }

        match insert_samples(&self.pool, &self.table, samples).await {
            Ok(rows) => {
                metrics::counter!("mysql_inserted_samples_total").increment(rows);
                info!(rows, "batch inserted into mysql");
                Ok(())
            }
            Err(err) => {
                warn!(error = %err, rows = samples.len(), "batch insert failed, buffering samples");
                self.buffer.push_many(samples)?;
                self.buffer.flush()?;
                metrics::counter!("buffered_samples_total").increment(samples.len() as u64);
                Ok(())
            }
        }
    }

    async fn replay_buffered(&self) -> Result<(), SinkError> {
        let entries = self.buffer.drain_batch(self.batch_size)?;
        if entries.is_empty() {
            return Ok(());
        }

        let samples = entries
            .iter()
            .map(|entry| entry.sample.clone())
            .collect::<Vec<_>>();
        match insert_samples(&self.pool, &self.table, &samples).await {
            Ok(rows) => {
                self.ack_buffered(&entries)?;
                metrics::counter!("buffer_replayed_samples_total").increment(rows);
                info!(rows, "buffered samples replayed into mysql");
            }
            Err(err) => {
                error!(error = %err, rows = samples.len(), "buffer replay failed");
            }
        }

        Ok(())
    }

    fn ack_buffered(&self, entries: &[BufferedSample]) -> Result<(), SinkError> {
        self.buffer.ack(entries)?;
        self.buffer.flush()?;
        Ok(())
    }
}

