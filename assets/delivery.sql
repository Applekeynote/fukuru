CREATE TABLE IF NOT EXISTS managed_deliveries (
 event_id TEXT PRIMARY KEY,
 entity_id TEXT NOT NULL,
 entity_version INTEGER NOT NULL,
 payload TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'QUEUED' CHECK(status IN ('QUEUED','LEASED','RETRY','DEAD','DELIVERED','SUPERSEDED')),
 attempts INTEGER NOT NULL DEFAULT 0,
 next_attempt INTEGER NOT NULL DEFAULT 0,
 lease_token TEXT,
 lease_until INTEGER,
 last_error TEXT,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 delivered_at INTEGER,
 UNIQUE(entity_id,entity_version)
);
CREATE INDEX IF NOT EXISTS managed_delivery_due ON managed_deliveries(status,next_attempt);
