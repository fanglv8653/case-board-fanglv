-- 2026-05-26 V0.1.12 · 抽取性能埋点
--
-- 目的:朋友实测产出数据,决定本地 OCR vs 云端 OCR 哪个更划算(快 + 准)。
-- 一份文档可能有多条 metric:文本抽取 + OCR(若兜底) + LLM 抽取 各一条。
--
-- 隐私:**不存** case_id / document_id / 路径,只存 filename 给作者识别"是什么类型的文件"。
-- 反馈通道拉最近 N 条进 MD,不带任何 PII。

CREATE TABLE extraction_metrics (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    -- 文件基础信息
    filename        TEXT    NOT NULL,            -- 仅文件名(无路径)
    ext             TEXT    NOT NULL,            -- pdf / docx / png / ...
    file_size_bytes INTEGER NOT NULL,
    -- 阶段
    stage           TEXT    NOT NULL,            -- text_extract / ocr / llm_extract
    backend         TEXT    NOT NULL,            -- pdf-inspector / pdftotext / textutil / read_direct
                                                 -- / mineru-precision / local-vision
                                                 -- / deepseek / local-llm
    outcome         TEXT    NOT NULL,            -- ok / failed / skipped
    elapsed_ms      INTEGER NOT NULL,
    text_chars      INTEGER,                     -- 抽出字数(失败 NULL)
    error_short     TEXT,                        -- 简短错误(失败才填,已脱敏)
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_extraction_metrics_created ON extraction_metrics(created_at DESC);
