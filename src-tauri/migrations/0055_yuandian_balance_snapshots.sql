-- 2026-07-29 · 元典 MCP 官方余额快照与本机积分账对账
--
-- 元典通过免费 MCP 工具 yuandian_get_user_balance 返回官方余额。这里只保存
-- API Key 的不可逆 SHA-256 截断指纹、余额和本机累计账本；绝不保存 API Key。

CREATE TABLE IF NOT EXISTS yuandian_balance_snapshots (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    key_fingerprint          TEXT NOT NULL,
    point_balance            INTEGER NOT NULL,
    count_balance            INTEGER NOT NULL DEFAULT 0,
    local_credits_total      INTEGER NOT NULL DEFAULT 0,
    local_api_calls_total    INTEGER NOT NULL DEFAULT 0,
    fetched_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_yuandian_balance_key_id
    ON yuandian_balance_snapshots(key_fingerprint, id DESC);
