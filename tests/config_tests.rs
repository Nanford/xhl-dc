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
  table: "ylk_alarm_log"
  tag_prefix_routes:
    FSC1:
      table: "cpk_alarm_log"
    FLK1:
      table: "flk_alarm_log"
    YLK1:
      table: "ylk_alarm_log"
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

    assert_eq!(config.opcua.as_ref().unwrap().endpoint, "opc.tcp://127.0.0.1:49320");
    assert_eq!(config.subscriptions[0].tags[0].alias, "temp_1");
    assert_eq!(config.sink.table, "ylk_alarm_log");
    assert_eq!(config.sink.tag_prefix_routes["FSC1"].table, "cpk_alarm_log");
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
    let yaml = valid_yaml().replace("table: \"ylk_alarm_log\"", "table: \"tag_log;drop\"");

    let err = AppConfig::from_yaml_str(&yaml).expect_err("table name must be a MySQL identifier");

    assert!(err.to_string().contains("table"));
}

#[test]
fn parses_enabled_opcua_discovery_config() {
    let yaml = valid_yaml().replace(
        "  application_uri: \"urn:KepwareBridge\"",
        r#"  application_uri: "urn:KepwareBridge"
  discovery:
    enabled: true
    target_subscription: "fast"
    root_node_ids:
      - "ns=2;s=Channel1"
    include_paths:
      - "Channel1.Device1"
    exclude_paths:
      - "Channel1.Device1._System"
    min_namespace_index: 2
    max_depth_count: 6
    max_tags_count: 200
    include_system: false
    include_arrays: false"#,
    );

    let config = AppConfig::from_yaml_str(&yaml).expect("discovery config should parse");
    let discovery = config.opcua.as_ref().unwrap().discovery.as_ref().expect("discovery config should be present");

    assert!(discovery.enabled);
    assert_eq!(discovery.target_subscription, "fast");
    assert_eq!(discovery.root_node_ids, vec!["ns=2;s=Channel1"]);
    assert_eq!(discovery.include_paths, vec!["Channel1.Device1"]);
    assert_eq!(discovery.exclude_paths, vec!["Channel1.Device1._System"]);
    assert_eq!(discovery.min_namespace_index, 2);
    assert_eq!(discovery.max_depth_count, 6);
    assert_eq!(discovery.max_tags_count, 200);
}

#[test]
fn parses_opcua_description_map_path() {
    let yaml = valid_yaml().replace(
        "  application_uri: \"urn:KepwareBridge\"",
        "  application_uri: \"urn:KepwareBridge\"\n  description_map_path: \"./tag_descriptions.yaml\"",
    );

    let config = AppConfig::from_yaml_str(&yaml).expect("description map path should parse");

    assert_eq!(
        config
            .opcua
            .as_ref()
            .and_then(|opcua| opcua.description_map_path.as_deref()),
        Some("./tag_descriptions.yaml")
    );
}

#[test]
fn parses_opcua_monitored_item_create_batch_size_count() {
    let yaml = valid_yaml().replace(
        "  application_uri: \"urn:KepwareBridge\"",
        "  application_uri: \"urn:KepwareBridge\"\n  monitored_item_create_batch_size_count: 250",
    );

    let config = AppConfig::from_yaml_str(&yaml).expect("batch size should parse");

    assert_eq!(
        config
            .opcua
            .as_ref()
            .map(|opcua| opcua.monitored_item_create_batch_size_count),
        Some(250)
    );
}

#[test]
fn rejects_zero_opcua_monitored_item_create_batch_size_count() {
    let yaml = valid_yaml().replace(
        "  application_uri: \"urn:KepwareBridge\"",
        "  application_uri: \"urn:KepwareBridge\"\n  monitored_item_create_batch_size_count: 0",
    );

    let err = AppConfig::from_yaml_str(&yaml)
        .expect_err("zero monitored item create batch size should be rejected");

    assert!(err
        .to_string()
        .contains("monitored_item_create_batch_size_count"));
}

#[test]
fn rejects_negative_opcua_monitored_item_create_batch_size_count() {
    let yaml = valid_yaml().replace(
        "  application_uri: \"urn:KepwareBridge\"",
        "  application_uri: \"urn:KepwareBridge\"\n  monitored_item_create_batch_size_count: -1",
    );

    let err = AppConfig::from_yaml_str(&yaml)
        .expect_err("negative monitored item create batch size should be rejected");

    assert!(err
        .to_string()
        .contains("monitored_item_create_batch_size_count"));
}

#[test]
fn rejects_enabled_discovery_with_invalid_root_node_id() {
    let yaml = valid_yaml().replace(
        "  application_uri: \"urn:KepwareBridge\"",
        r#"  application_uri: "urn:KepwareBridge"
  discovery:
    enabled: true
    root_node_ids:
      - "Channel1""#,
    );

    let err = AppConfig::from_yaml_str(&yaml)
        .expect_err("enabled discovery root_node_ids must use OPC UA NodeId syntax");

    assert!(err.to_string().contains("root_node_ids"));
}

#[test]
fn rejects_enabled_discovery_with_zero_limits() {
    let yaml = valid_yaml().replace(
        "  application_uri: \"urn:KepwareBridge\"",
        r#"  application_uri: "urn:KepwareBridge"
  discovery:
    enabled: true
    max_depth_count: 0"#,
    );

    let err =
        AppConfig::from_yaml_str(&yaml).expect_err("enabled discovery limits must be positive");

    assert!(err.to_string().contains("max_depth_count"));
}

#[test]
fn rejects_enabled_discovery_with_unknown_target_subscription() {
    let yaml = valid_yaml().replace(
        "  application_uri: \"urn:KepwareBridge\"",
        r#"  application_uri: "urn:KepwareBridge"
  discovery:
    enabled: true
    target_subscription: "missing""#,
    );

    let err = AppConfig::from_yaml_str(&yaml)
        .expect_err("enabled discovery target_subscription must exist");

    assert!(err.to_string().contains("target_subscription"));
}

fn wcs_only_yaml() -> &'static str {
    r#"
mysql:
  url: "mysql://user:pass@127.0.0.1:3306/iot"
  max_connections: 8

wcs:
  base_url: "http://192.168.1.100:8080/api/v1"
  poll_interval_ms: 5000
  timeout_ms: 10000
  endpoints:
    - path: "/conveyor/status"
      method: GET
      tags:
        - { json_path: "running", alias: "conveyor_running" }
        - { json_path: "speed", alias: "conveyor_speed", value_type: float }

sink:
  table: "ylk_alarm_log"
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
fn parses_wcs_only_config() {
    let config = AppConfig::from_yaml_str(wcs_only_yaml()).expect("wcs-only config should parse");

    assert!(config.opcua.is_none());
    let wcs = config.wcs.as_ref().unwrap();
    assert_eq!(wcs.base_url, "http://192.168.1.100:8080/api/v1");
    assert_eq!(wcs.endpoints.len(), 1);
    assert_eq!(wcs.endpoints[0].tags.len(), 2);
    assert_eq!(wcs.endpoints[0].tags[0].alias, "conveyor_running");
}

#[test]
fn parses_opcua_and_wcs_config() {
    let yaml = format!(
        "{}\nwcs:\n  base_url: \"http://wcs:8080\"\n  endpoints:\n    - path: \"/status\"\n      tags:\n        - {{ json_path: \"ok\", alias: \"wcs_ok\" }}",
        valid_yaml()
    );

    let config = AppConfig::from_yaml_str(&yaml).expect("dual config should parse");

    assert!(config.opcua.is_some());
    assert!(config.wcs.is_some());
}

#[test]
fn rejects_config_with_no_collector() {
    let yaml = r#"
mysql:
  url: "mysql://user:pass@127.0.0.1:3306/iot"

sink:
  table: "ylk_alarm_log"
  batch_size: 500
  flush_interval_ms: 1000

buffer:
  path: "./data/wal"
  max_size_mb: 1024

metrics:
  bind: "127.0.0.1:9090"
"#;

    let err = AppConfig::from_yaml_str(yaml).expect_err("no collector should be rejected");

    assert!(err.to_string().contains("at least one collector"));
}

#[test]
fn rejects_wcs_with_empty_base_url() {
    let yaml = wcs_only_yaml().replace(
        "base_url: \"http://192.168.1.100:8080/api/v1\"",
        "base_url: \"\"",
    );

    let err = AppConfig::from_yaml_str(&yaml).expect_err("empty base_url should be rejected");

    assert!(err.to_string().contains("base_url"));
}

#[test]
fn rejects_wcs_with_empty_endpoints() {
    let yaml = r#"
mysql:
  url: "mysql://user:pass@127.0.0.1:3306/iot"

wcs:
  base_url: "http://wcs:8080"
  endpoints: []

sink:
  table: "ylk_alarm_log"
  batch_size: 500
  flush_interval_ms: 1000

buffer:
  path: "./data/wal"
  max_size_mb: 1024

metrics:
  bind: "127.0.0.1:9090"
"#;

    let err = AppConfig::from_yaml_str(yaml).expect_err("empty endpoints should be rejected");

    assert!(err.to_string().contains("endpoints"));
}

#[test]
fn rejects_wcs_with_empty_tag_alias() {
    let yaml = wcs_only_yaml().replace("alias: \"conveyor_running\"", "alias: \"\"");

    let err = AppConfig::from_yaml_str(&yaml).expect_err("empty alias should be rejected");

    assert!(err.to_string().contains("alias"));
}
