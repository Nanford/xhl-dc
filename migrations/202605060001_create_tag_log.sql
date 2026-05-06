CREATE TABLE IF NOT EXISTS tag_log (
  id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  node_id VARCHAR(255) NOT NULL,
  alias VARCHAR(64) NOT NULL,
  value_type TINYINT NOT NULL,
  value_num DOUBLE NULL,
  value_str VARCHAR(512) NULL,
  source_ts DATETIME(3) NOT NULL,
  server_ts DATETIME(3) NOT NULL,
  quality INT NOT NULL,
  ingest_ts DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  INDEX idx_node_ts (node_id, source_ts DESC),
  INDEX idx_alias_ts (alias, source_ts DESC)
) ENGINE=InnoDB ROW_FORMAT=DYNAMIC;
