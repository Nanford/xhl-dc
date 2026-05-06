use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{TimeZone, Utc};
use kepware_bridge::buffer::SampleBuffer;
use kepware_bridge::types::{TagSample, ValueKind};

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("kepware_bridge_{name}_{unique}"))
}

fn sample(alias: &str, value: f64) -> TagSample {
    let ts = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    TagSample {
        node_id: format!("ns=2;s=Channel1.Device1.{alias}"),
        alias: alias.to_string(),
        value: ValueKind::Float(value),
        source_ts: ts,
        server_ts: ts,
        quality: 0,
    }
}

#[test]
fn persists_samples_until_ack() {
    let dir = temp_dir("buffer_ack");
    let buffer = SampleBuffer::open(&dir, 10).expect("buffer should open");
    buffer
        .push_many(&[sample("Temperature", 10.0), sample("Pressure", 20.0)])
        .expect("samples should be buffered");
    buffer.flush().expect("flush should succeed");
    drop(buffer);

    let buffer = SampleBuffer::open(&dir, 10).expect("buffer should reopen");
    let entries = buffer.drain_batch(10).expect("entries should drain");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].sample.alias, "Temperature");

    buffer.ack(&entries).expect("ack should remove entries");
    assert_eq!(buffer.len(), 0);

    let _ = fs::remove_dir_all(dir);
}
