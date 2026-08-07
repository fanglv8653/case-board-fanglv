-- v0.8.3 device-sync quarantine lifecycle and attempt/success timestamps.

ALTER TABLE device_sync_groups ADD COLUMN last_attempt_at TEXT;
ALTER TABLE device_sync_groups ADD COLUMN last_success_at TEXT;
ALTER TABLE device_sync_groups ADD COLUMN auto_paused INTEGER NOT NULL DEFAULT 0
    CHECK(auto_paused IN (0,1));
ALTER TABLE device_sync_groups ADD COLUMN pause_reason_code TEXT;

UPDATE device_sync_groups
SET last_attempt_at = last_synced_at,
    last_success_at = last_synced_at;

-- v0.8.2 ordered same-millisecond operations by (logical_time, operation_id).
-- Freeze that exact historical order once, then require every new local capture
-- to claim a transactionally monotonic sequence instead of relying on UUIDs.
ALTER TABLE device_sync_outbox ADD COLUMN capture_sequence INTEGER NOT NULL DEFAULT 0
    CHECK(capture_sequence >= 0);

WITH normalized AS (
    SELECT operation_id,
           ROW_NUMBER() OVER (
               PARTITION BY group_id
               ORDER BY logical_time, operation_id
           ) AS normalized_sequence
    FROM device_sync_outbox
)
UPDATE device_sync_outbox
SET capture_sequence = (
    SELECT normalized_sequence
    FROM normalized
    WHERE normalized.operation_id = device_sync_outbox.operation_id
);

CREATE UNIQUE INDEX idx_device_sync_outbox_capture_sequence
ON device_sync_outbox(group_id, capture_sequence);

CREATE INDEX idx_device_sync_outbox_pending_capture
ON device_sync_outbox(group_id, state, capture_sequence);

ALTER TABLE device_sync_quarantine RENAME TO device_sync_quarantine_legacy;

CREATE TABLE device_sync_quarantine (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT,
    source_path TEXT,
    source_device_id TEXT NOT NULL,
    source_sequence INTEGER NOT NULL,
    reason_code TEXT NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active','resolved','manual_review')),
    first_seen_at TEXT NOT NULL DEFAULT(datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT(datetime('now')),
    retry_count INTEGER NOT NULL DEFAULT 1 CHECK(retry_count >= 1),
    resolved_at TEXT,
    last_error_code TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE SET NULL
);

-- Preserve every historical row, but do not invent a real package identity for
-- legacy rows that never recorded device/sequence. They remain visible for
-- manual review and never participate in the active package unique key.
INSERT INTO device_sync_quarantine (
    id, group_id, source_path, source_device_id, source_sequence,
    reason_code, details_json, status,
    first_seen_at, last_seen_at, retry_count, resolved_at,
    last_error_code, created_at
)
SELECT legacy.id,
       legacy.group_id,
       NULL,
       '__legacy__',
       -1,
       legacy.reason_code,
       '{"legacy_record":true,"identity":"unknown","sensitive_content":"redacted"}',
       'manual_review',
       legacy.created_at,
       legacy.created_at,
       1,
       NULL,
       legacy.reason_code,
       legacy.created_at
FROM device_sync_quarantine_legacy AS legacy;

DROP TABLE device_sync_quarantine_legacy;

CREATE UNIQUE INDEX idx_device_sync_quarantine_active_key
ON device_sync_quarantine(
    COALESCE(group_id,''), source_device_id, source_sequence, reason_code
)
WHERE status='active';

CREATE INDEX idx_device_sync_quarantine_group_status
ON device_sync_quarantine(group_id, status, last_seen_at DESC);

-- A prepared export is committed before either NAS object is published.  It
-- contains only serialized encrypted envelopes and the minimum metadata needed
-- to prove that a retry still refers to the same local outbox package.
CREATE TABLE device_sync_export_drafts (
    group_id TEXT NOT NULL,
    local_device_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence >= 1),
    key_epoch INTEGER NOT NULL CHECK(key_epoch >= 1),
    previous_manifest_hash TEXT,
    event_envelope_bytes BLOB NOT NULL,
    manifest_envelope_bytes BLOB NOT NULL,
    event_ciphertext_sha256 TEXT NOT NULL,
    manifest_ciphertext_sha256 TEXT NOT NULL,
    operation_ids_json TEXT NOT NULL,
    operation_fingerprint TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'prepared'
        CHECK(state IN ('prepared','finalized')),
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    finalized_at TEXT,
    PRIMARY KEY(group_id, local_device_id, sequence),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);

CREATE INDEX idx_device_sync_export_drafts_state
ON device_sync_export_drafts(group_id, local_device_id, state, sequence);

CREATE UNIQUE INDEX idx_device_sync_export_drafts_one_prepared
ON device_sync_export_drafts(group_id)
WHERE state='prepared';
