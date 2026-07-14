CREATE TABLE IF NOT EXISTS lpr_rate_cache (
    publication_date TEXT PRIMARY KEY,
    lpr_1y REAL NOT NULL CHECK (lpr_1y > 0 AND lpr_1y < 20),
    lpr_5y REAL NOT NULL CHECK (lpr_5y > 0 AND lpr_5y < 20),
    source_url TEXT NOT NULL,
    fetched_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS lpr_refresh_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_attempt_at TEXT,
    last_attempt_cst_date TEXT,
    last_success_at TEXT,
    latest_published_date TEXT,
    last_error TEXT
);

INSERT OR IGNORE INTO lpr_refresh_state (id) VALUES (1);
