use std::collections::{BTreeMap, HashMap};
use std::mem;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use sqlx::{mysql::MySqlPoolOptions, MySqlPool};
use thiserror::Error;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

use crate::buffer::{BufferedSample, SampleBuffer};
use crate::config::{MysqlConfig, SinkConfig, SubscriptionConfig, TagConfig};
use crate::metadata::TagMetadataCache;
use crate::types::{TagSample, ValueKind};

const REALTIME_WRITE_BATCH_SIZE: usize = 500;

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

    let placeholders = std::iter::repeat_n("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", rows)
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!(
        "INSERT INTO `{table}` \
         (`location`, `device`, `device_id`, `node_id`, `alias`, `tag`, `fault_type`, `tag_state`, `tag_value`, `description`, `remark`, `create_at`, `update_at`) \
         VALUES {placeholders}"
    ))
}

pub fn build_realtime_seed_sql(rows: usize) -> Result<String, SinkError> {
    if rows == 0 {
        return Err(SinkError::EmptyInsert);
    }

    let placeholders = std::iter::repeat_n("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", rows)
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!(
        "INSERT INTO `device_realtime_status` \
         (`source_system`, `source_table`, `location`, `device_type`, `device_id`, `node_id`, `alias`, `tag`, `fault_type`, `description`) \
         VALUES {placeholders} \
         ON DUPLICATE KEY UPDATE \
         `source_system` = VALUES(`source_system`), \
         `location` = VALUES(`location`), \
         `device_type` = VALUES(`device_type`), \
         `device_id` = VALUES(`device_id`), \
         `alias` = VALUES(`alias`), \
         `tag` = VALUES(`tag`), \
         `fault_type` = VALUES(`fault_type`), \
         `description` = VALUES(`description`)"
    ))
}

pub fn build_realtime_upsert_sql(rows: usize) -> Result<String, SinkError> {
    if rows == 0 {
        return Err(SinkError::EmptyInsert);
    }

    let placeholders = std::iter::repeat_n("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", rows)
        .collect::<Vec<_>>()
        .join(", ");
    let accepts_newer = "`last_fault_at` IS NULL OR VALUES(`last_fault_at`) >= `last_fault_at`";

    Ok(format!(
        "INSERT INTO `device_realtime_status` \
         (`source_system`, `source_table`, `location`, `device_type`, `device_id`, `node_id`, `alias`, `tag`, `fault_type`, `tag_state`, `tag_value`, `description`, `status_description`, `last_fault_at`) \
         VALUES {placeholders} \
         ON DUPLICATE KEY UPDATE \
         `source_system` = IF({accepts_newer}, VALUES(`source_system`), `source_system`), \
         `location` = IF({accepts_newer}, VALUES(`location`), `location`), \
         `device_type` = IF({accepts_newer}, VALUES(`device_type`), `device_type`), \
         `device_id` = IF({accepts_newer}, VALUES(`device_id`), `device_id`), \
         `alias` = IF({accepts_newer}, VALUES(`alias`), `alias`), \
         `tag` = IF({accepts_newer}, VALUES(`tag`), `tag`), \
         `fault_type` = IF({accepts_newer}, VALUES(`fault_type`), `fault_type`), \
         `tag_state` = IF({accepts_newer}, VALUES(`tag_state`), `tag_state`), \
         `tag_value` = IF({accepts_newer}, VALUES(`tag_value`), `tag_value`), \
         `description` = IF({accepts_newer}, VALUES(`description`), `description`), \
         `status_description` = IF({accepts_newer}, VALUES(`status_description`), `status_description`), \
         `last_fault_at` = IF({accepts_newer}, VALUES(`last_fault_at`), `last_fault_at`), \
         `updated_at` = IF({accepts_newer}, CURRENT_TIMESTAMP(3), `updated_at`)"
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
pub struct RealtimeTagSeed {
    pub source_table: String,
    pub location: String,
    pub device_type: String,
    pub device_id: String,
    pub node_id: String,
    pub alias: String,
    pub tag: String,
    pub fault_type: Option<String>,
    pub description: String,
}

pub fn realtime_tag_seed_for_subscription(
    router: &SinkTableRouter,
    subscription: &SubscriptionConfig,
    tag: &TagConfig,
    metadata_cache: &TagMetadataCache,
) -> RealtimeTagSeed {
    let now = Utc::now();
    let sample = TagSample::new(
        tag.node_id.clone(),
        tag.alias.clone(),
        subscription.area.clone(),
        tag.device.clone(),
        tag.device_id.clone(),
        tag.description.clone(),
        ValueKind::Int(0),
        now,
        now,
        0,
        "opcua",
    );
    let fields = sample.alarm_log_fields();
    let metadata = metadata_cache.lookup(&sample, &fields);

    RealtimeTagSeed {
        source_table: router.table_for_sample(&sample).to_string(),
        location: fields.location,
        device_type: fields.device,
        device_id: fields.device_id,
        node_id: tag.node_id.clone(),
        alias: tag.alias.clone(),
        tag: fields.tag,
        fault_type: metadata.and_then(|metadata| metadata.fault_type.clone()),
        description: fields.description,
    }
}

fn realtime_tag_seeds_for_subscriptions(
    router: &SinkTableRouter,
    subscriptions: &[SubscriptionConfig],
    metadata_cache: &TagMetadataCache,
) -> Vec<RealtimeTagSeed> {
    subscriptions
        .iter()
        .flat_map(|subscription| {
            subscription.tags.iter().map(move |tag| {
                realtime_tag_seed_for_subscription(router, subscription, tag, metadata_cache)
            })
        })
        .collect()
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

    pub fn from_routes<I, K, T>(
        default_table: impl Into<String>,
        routes: I,
    ) -> Result<Self, SinkError>
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
    let metadata = TagMetadataCache::default();
    for (table, group) in group_samples_by_table(router, samples) {
        inserted += insert_sample_refs(pool, &table, &group, &metadata).await?;
    }
    Ok(inserted)
}

async fn insert_sample_refs(
    pool: &MySqlPool,
    table: &str,
    samples: &[&TagSample],
    metadata_cache: &TagMetadataCache,
) -> Result<u64, SinkError> {
    let rows = insert_history_sample_refs(pool, table, samples, metadata_cache).await?;
    if let Err(err) = upsert_realtime_sample_refs(pool, table, samples, metadata_cache).await {
        warn!(
            error = %err,
            table = %table,
            rows = samples.len(),
            "history insert succeeded but realtime status update failed"
        );
        metrics::counter!("realtime_upsert_failed_total").increment(samples.len() as u64);
    }
    Ok(rows)
}

async fn insert_history_sample_refs(
    pool: &MySqlPool,
    table: &str,
    samples: &[&TagSample],
    metadata_cache: &TagMetadataCache,
) -> Result<u64, SinkError> {
    if samples.is_empty() {
        return Ok(0);
    }

    let sql = build_insert_sql(table, samples.len())?;
    let mut query = sqlx::query(&sql);
    let create_time_source = Utc::now();
    for sample in samples {
        let fields = sample.alarm_log_fields();
        let metadata = metadata_cache.lookup(sample, &fields);
        if metadata.is_none() && !metadata_cache.is_empty() {
            metrics::counter!("metadata_unmapped_samples_total").increment(1);
        }
        let (create_at, update_at) =
            alarm_log_mysql_timestamps(sample.source_ts, create_time_source);
        query = query
            .bind(optional_text(&fields.location))
            .bind(optional_text(&fields.device))
            .bind(optional_text(&fields.device_id))
            .bind(optional_text(&sample.node_id))
            .bind(optional_text(&sample.alias))
            .bind(fields.tag)
            .bind(metadata.and_then(|metadata| metadata.fault_type.as_deref()))
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

async fn seed_realtime_tag_refs(
    pool: &MySqlPool,
    seeds: &[RealtimeTagSeed],
) -> Result<u64, SinkError> {
    let mut affected = 0;
    for chunk in seeds.chunks(REALTIME_WRITE_BATCH_SIZE) {
        let sql = build_realtime_seed_sql(chunk.len())?;
        let mut query = sqlx::query(&sql);
        for seed in chunk {
            query = query
                .bind("opcua")
                .bind(seed.source_table.as_str())
                .bind(optional_text(&seed.location))
                .bind(optional_text(&seed.device_type))
                .bind(optional_text(&seed.device_id))
                .bind(optional_text(&seed.node_id))
                .bind(optional_text(&seed.alias))
                .bind(seed.tag.as_str())
                .bind(seed.fault_type.as_deref())
                .bind(optional_text(&seed.description));
        }
        affected += query.execute(pool).await?.rows_affected();
    }
    Ok(affected)
}

async fn upsert_realtime_sample_refs(
    pool: &MySqlPool,
    table: &str,
    samples: &[&TagSample],
    metadata_cache: &TagMetadataCache,
) -> Result<u64, SinkError> {
    if samples.is_empty() {
        return Ok(0);
    }

    let mut affected = 0;
    for chunk in samples.chunks(REALTIME_WRITE_BATCH_SIZE) {
        let sql = build_realtime_upsert_sql(chunk.len())?;
        let mut query = sqlx::query(&sql);
        for sample in chunk {
            let fields = sample.alarm_log_fields();
            let metadata = metadata_cache.lookup(sample, &fields);
            let last_fault_at = alarm_log_mysql_datetime(sample.source_ts);
            query = query
                .bind(source_system(&sample.source))
                .bind(table)
                .bind(optional_text(&fields.location))
                .bind(optional_text(&fields.device))
                .bind(optional_text(&fields.device_id))
                .bind(optional_text(&sample.node_id))
                .bind(optional_text(&sample.alias))
                .bind(fields.tag)
                .bind(metadata.and_then(|metadata| metadata.fault_type.as_deref()))
                .bind(sample.tag_state())
                .bind(sample.tag_value())
                .bind(optional_text(&fields.description))
                .bind(optional_text(&fields.remark))
                .bind(last_fault_at);
        }
        affected += query.execute(pool).await?.rows_affected();
    }
    Ok(affected)
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

fn source_system(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "opcua"
    } else {
        trimmed
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
    metadata_cache: TagMetadataCache,
    realtime_tag_seeds: Vec<RealtimeTagSeed>,
    batch_size: usize,
    flush_interval: Duration,
}

pub struct SinkWorkerSettings {
    pub batch_size: usize,
    pub flush_interval: Duration,
    pub subscriptions: Vec<SubscriptionConfig>,
}

impl SinkWorker {
    pub fn new(
        pool: MySqlPool,
        router: SinkTableRouter,
        receiver: mpsc::Receiver<TagSample>,
        shutdown: watch::Receiver<bool>,
        buffer: SampleBuffer,
        metadata_cache: TagMetadataCache,
        settings: SinkWorkerSettings,
    ) -> Self {
        let realtime_tag_seeds =
            realtime_tag_seeds_for_subscriptions(&router, &settings.subscriptions, &metadata_cache);
        Self {
            pool,
            router,
            receiver,
            shutdown,
            buffer,
            metadata_cache,
            realtime_tag_seeds,
            batch_size: settings.batch_size,
            flush_interval: settings.flush_interval,
        }
    }

    pub async fn run(mut self) -> Result<(), SinkError> {
        info!(
            default_table = %self.router.default_table(),
            route_count = self.router.route_count(),
            metadata_entries = self.metadata_cache.len(),
            realtime_seed_tags = self.realtime_tag_seeds.len(),
            batch_size = self.batch_size,
            flush_interval_ms = self.flush_interval.as_millis(),
            "sink worker started"
        );

        let mut batcher = BatchBuilder::new(self.batch_size);
        let mut interval = tokio::time::interval(self.flush_interval);
        self.seed_realtime_status().await;
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
            match insert_sample_refs(&self.pool, &table, &group, &self.metadata_cache).await {
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
                    let failed = group
                        .iter()
                        .map(|sample| (*sample).clone())
                        .collect::<Vec<_>>();
                    self.buffer.push_many(&failed)?;
                    self.buffer.flush()?;
                    metrics::counter!("buffered_samples_total").increment(failed.len() as u64);
                }
            }
        }
        Ok(())
    }

    async fn seed_realtime_status(&self) {
        if self.realtime_tag_seeds.is_empty() {
            return;
        }

        match seed_realtime_tag_refs(&self.pool, &self.realtime_tag_seeds).await {
            Ok(rows) => {
                metrics::counter!("realtime_seeded_tags_total").increment(rows);
                info!(
                    rows,
                    tags = self.realtime_tag_seeds.len(),
                    "realtime status table seeded from subscriptions"
                );
            }
            Err(err) => {
                warn!(
                    error = %err,
                    tags = self.realtime_tag_seeds.len(),
                    "failed to seed realtime status table from subscriptions"
                );
                metrics::counter!("realtime_seed_failed_total")
                    .increment(self.realtime_tag_seeds.len() as u64);
            }
        }
    }

    async fn replay_buffered(&self) -> Result<(), SinkError> {
        let entries = self.buffer.drain_batch(self.batch_size)?;
        if entries.is_empty() {
            return Ok(());
        }

        for (table, group) in group_buffered_by_table(&self.router, &entries) {
            let samples = group.iter().map(|entry| &entry.sample).collect::<Vec<_>>();
            match insert_sample_refs(&self.pool, &table, &samples, &self.metadata_cache).await {
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
