-- 法院申报账号已迁入操作系统凭据库，不再属于任务审计数据。
-- 保留任务、状态、法院、时间等非敏感审计字段，仅清除历史明文账号。
UPDATE court_filing_jobs
SET cookie_account = NULL
WHERE cookie_account IS NOT NULL;
