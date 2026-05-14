use std::collections::{BTreeMap, HashMap};
use std::mem;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use sqlx::{mysql::MySqlPoolOptions, MySqlPool};
use thiserror::Error;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

use crate::buffer::{BufferedSample, SampleBuffer};
use crate::config::{MysqlConfig, SinkConfig};
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

    let placeholders = std::iter::repeat("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .take(rows)
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!(
        "INSERT INTO `{table}` \
         (`location`, `device`, `device_id`, `tag`, `tag_state`, `tag_value`, `description`, `remark`, `create_at`, `update_at`) \
         VALUES {placeholders}"
    ))
}

pub fn alarm_log_mysql_datetime(timestamp: DateTime<Utc>) -> NaiveDateTime {
    let beijing_offset =
        FixedOffset::east_opt(8 * 60 * 60).expect("Beijing offset is a valid fixed offset");
    timestamp.with_timezone(&beijing_offset).naive_local()
}

pub fn alarm_log_mysql_timestamps(
    source_ts: DateTime<Utc>,
    system_now: DateTime<Utc>,
) -> (NaiveDateTime, NaiveDateTime) {
    (
        alarm_log_mysql_datetime(system_now),
        alarm_log_mysql_datetime(source_ts),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkTableRoute {
    pub table: String,
}

#[derive(Debug, Clone)]
pub struct SinkTableRouter {
    default_table: String,
    routes: HashMap<String, SinkTableRoute>,
}

impl SinkTableRouter {
    pub fn from_config(config: &SinkConfig) -> Result<Self, SinkError> {
        let routes = config
            .tag_prefix_routes
            .iter()
            .map(|(prefix, route)| {
                (
                    prefix.clone(),
                    SinkTableRoute {
                        table: route.table.clone(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        Self::new(config.table.clone(), routes)
    }

    pub fn from_routes<I, K, T>(default_table: impl Into<String>, routes: I) -> Result<Self, SinkError>
    where
        I: IntoIterator<Item = (K, T)>,
        K: Into<String>,
        T: Into<String>,
    {
        let routes = routes
            .into_iter()
            .map(|(prefix, table)| {
                (
                    prefix.into(),
                    SinkTableRoute {
                        table: table.into(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        Self::new(default_table, routes)
    }

    pub fn new(
        default_table: impl Into<String>,
        routes: HashMap<String, SinkTableRoute>,
    ) -> Result<Self, SinkError> {
        let default_table = default_table.into();
        validate_mysql_identifier(&default_table)?;
        for route in routes.values() {
            validate_mysql_identifier(&route.table)?;
        }
        Ok(Self {
            default_table,
            routes,
        })
    }

    pub fn table_for_sample<'a>(&'a self, sample: &'a TagSample) -> &'a str {
        sample
            .tag_prefix()
            .and_then(|prefix| self.routes.get(prefix))
            .map(|route| route.table.as_str())
            .unwrap_or(self.default_table.as_str())
    }

    pub fn route_count(&self) -> usize {
        self.routes.len()
    }

    pub fn default_table(&self) -> &str {
        &self.default_table
    }
}

pub async fn connect_mysql(config: &MysqlConfig) -> Result<MySqlPool, sqlx::Error> {
    MySqlPoolOptions::new()
        .max_connections(config.max_connections)
        .connect(&config.url)
        .await
}

pub async fn insert_samples(
    pool: &MySqlPool,
    router: &SinkTableRouter,
    samples: &[TagSample],
) -> Result<u64, SinkError> {
    if samples.is_empty() {
        return Ok(0);
    }

    let mut inserted = 0;
    for (table, group) in group_samples_by_table(router, samples) {
        inserted += insert_sample_refs(pool, &table, &group).await?;
    }
    Ok(inserted)
}

async fn insert_sample_refs(
    pool: &MySqlPool,
    table: &str,
    samples: &[&TagSample],
) -> Result<u64, SinkError> {
    if samples.is_empty() {
        return Ok(0);
    }

    let sql = build_insert_sql(table, samples.len())?;
    let mut query = sqlx::query(&sql);
    let create_time_source = Utc::now();
    for sample in samples {
        let fields = sample.alarm_log_fields();
        let (create_at, update_at) = alarm_log_mysql_timestamps(sample.source_ts, create_time_source);
        query = query
            .bind(optional_text(&fields.location))
            .bind(optional_text(&fields.device))
            .bind(optional_text(&fields.device_id))
            .bind(fields.tag)
            .bind(sample.tag_state())
            .bind(sample.tag_value())
            .bind(optional_text(&fields.description))
            .bind(optional_text(&fields.remark))
            .bind(create_at)
            .bind(update_at);
    }

    let result = query.execute(pool).await?;
    Ok(result.rows_affected())
}

fn group_samples_by_table<'a>(
    router: &'a SinkTableRouter,
    samples: &'a [TagSample],
) -> BTreeMap<String, Vec<&'a TagSample>> {
    let mut groups = BTreeMap::new();
    for sample in samples {
        groups
            .entry(router.table_for_sample(sample).to_string())
            .or_insert_with(Vec::new)
            .push(sample);
    }
    groups
}

fn group_buffered_by_table<'a>(
    router: &'a SinkTableRouter,
    entries: &'a [BufferedSample],
) -> BTreeMap<String, Vec<&'a BufferedSample>> {
    let mut groups = BTreeMap::new();
    for entry in entries {
        groups
            .entry(router.table_for_sample(&entry.sample).to_string())
            .or_insert_with(Vec::new)
            .push(entry);
    }
    groups
}

fn optional_text(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_string())
    }
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
    router: SinkTableRouter,
    receiver: mpsc::Receiver<TagSample>,
    shutdown: watch::Receiver<bool>,
    buffer: SampleBuffer,
    batch_size: usize,
    flush_interval: Duration,
}

impl SinkWorker {
    pub fn new(
        pool: MySqlPool,
        router: SinkTableRouter,
        receiver: mpsc::Receiver<TagSample>,
        shutdown: watch::Receiver<bool>,
        buffer: SampleBuffer,
        batch_size: usize,
        flush_interval: Duration,
    ) -> Self {
        Self {
            pool,
            router,
            receiver,
            shutdown,
            buffer,
            batch_size,
            flush_interval,
        }
    }

    pub async fn run(mut self) -> Result<(), SinkError> {
        info!(
            default_table = %self.router.default_table(),
            route_count = self.router.route_count(),
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

        for (table, group) in group_samples_by_table(&self.router, samples) {
            match insert_sample_refs(&self.pool, &table, &group).await {
                Ok(rows) => {
                    metrics::counter!("mysql_inserted_samples_total").increment(rows);
                    info!(rows, table = %table, "batch inserted into mysql");
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        table = %table,
                        rows = group.len(),
                        "batch insert failed, buffering samples"
                    );
                    let failed = group.iter().map(|sample| (*sample).clone()).collect::<Vec<_>>();
                    self.buffer.push_many(&failed)?;
                    self.buffer.flush()?;
                    metrics::counter!("buffered_samples_total").increment(failed.len() as u64);
                }
            }
        }
        Ok(())
    }

    async fn replay_buffered(&self) -> Result<(), SinkError> {
        let entries = self.buffer.drain_batch(self.batch_size)?;
        if entries.is_empty() {
            return Ok(());
        }

        for (table, group) in group_buffered_by_table(&self.router, &entries) {
            let samples = group.iter().map(|entry| &entry.sample).collect::<Vec<_>>();
            match insert_sample_refs(&self.pool, &table, &samples).await {
                Ok(rows) => {
                    let acked = group
                        .iter()
                        .map(|entry| (*entry).clone())
                        .collect::<Vec<_>>();
                    self.ack_buffered(&acked)?;
                    metrics::counter!("buffer_replayed_samples_total").increment(rows);
                    info!(rows, table = %table, "buffered samples replayed into mysql");
                }
                Err(err) => {
                    error!(
                        error = %err,
                        table = %table,
                        rows = samples.len(),
                        "buffer replay failed"
                    );
                }
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
