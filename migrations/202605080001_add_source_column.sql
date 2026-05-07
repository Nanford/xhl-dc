ALTER TABLE tag_log
  ADD COLUMN source VARCHAR(32) NOT NULL DEFAULT 'opcua' AFTER quality;

CREATE INDEX idx_source_ts ON tag_log (source, source_ts DESC);
