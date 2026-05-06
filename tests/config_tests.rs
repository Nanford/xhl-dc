use kepware_bridge::config::AppConfig;

fn valid_yaml() -> &'static str {
    r#"
opcua:
  endpoint: "opc.tcp://127.0.0.1:49320"
  security_policy: None
  identity: anonymous
  session_retry_limit: -1
  application_uri: "urn:KepwareBridge"

mysql:
  url: "mysql://user:pass@127.0.0.1:3306/iot"
  max_connections: 8

subscriptions:
  - name: "fast"
    publishing_interval_ms: 500
    keep_alive_count: 10
    lifetime_count: 30
    tags:
      - { node_id: "ns=2;s=Channel1.Device1.Temperature", alias: "temp_1" }

sink:
  table: "tag_log"
  batch_size: 500
  flush_interval_ms: 1000

buffer:
  path: "./data/wal"
  max_size_mb: 1024

metrics:
  bind: "127.0.0.1:9090"
"#
}

#[test]
fn parses_valid_config() {
    let config = AppConfig::from_yaml_str(valid_yaml()).expect("valid config should parse");

    assert_eq!(config.opcua.endpoint, "opc.tcp://127.0.0.1:49320");
    assert_eq!(config.subscriptions[0].tags[0].alias, "temp_1");
    assert_eq!(config.sink.table, "tag_log");
}

#[test]
fn parses_windows_utf8_bom_config() {
    let yaml = format!("\u{feff}{}", valid_yaml());

    let config = AppConfig::from_yaml_str(&yaml).expect("Windows UTF-8 BOM should be accepted");

    assert_eq!(config.mysql.max_connections, 8);
}

#[test]
fn rejects_lifetime_count_below_keepalive_ratio() {
    let yaml = valid_yaml().replace("lifetime_count: 30", "lifetime_count: 20");

    let err = AppConfig::from_yaml_str(&yaml).expect_err("lifetime_count must be validated");

    assert!(err.to_string().contains("lifetime_count"));
}

#[test]
fn rejects_invalid_node_id() {
    let yaml = valid_yaml().replace(
        "ns=2;s=Channel1.Device1.Temperature",
        "Channel1.Device1.Temperature",
    );

    let err = AppConfig::from_yaml_str(&yaml).expect_err("Kepware NodeId must use OPC UA syntax");

    assert!(err.to_string().contains("node_id"));
}

#[test]
fn rejects_unsafe_table_name() {
    let yaml = valid_yaml().replace("table: \"tag_log\"", "table: \"tag_log;drop\"");

    let err = AppConfig::from_yaml_str(&yaml).expect_err("table name must be a MySQL identifier");

    assert!(err.to_string().contains("table"));
}
