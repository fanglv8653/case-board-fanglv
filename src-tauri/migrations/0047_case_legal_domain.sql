-- 案件法律领域与展示名称单一事实源。
-- case_type 是历史业务类型（如“诉讼/非诉”），不再被当作民事/刑事领域。
ALTER TABLE cases ADD COLUMN legal_domain TEXT NOT NULL DEFAULT 'unknown'
    CHECK (legal_domain IN ('criminal', 'civil', 'other', 'unknown'));

ALTER TABLE cases ADD COLUMN domain_source TEXT NOT NULL DEFAULT 'legacy'
    CHECK (domain_source IN ('manual', 'inferred', 'legacy'));

ALTER TABLE cases ADD COLUMN display_name_override TEXT;

-- 旧 case_type 本身明确标注领域时，记为 legacy；通用的“诉讼”不猜领域。
UPDATE cases
SET legal_domain = CASE
        WHEN lower(trim(case_type)) = 'criminal' OR case_type LIKE '%刑事%' THEN 'criminal'
        WHEN lower(trim(case_type)) IN ('civil', 'arbitration')
             OR case_type LIKE '%民事%'
             OR case_type LIKE '%仲裁%' THEN 'civil'
        WHEN lower(trim(case_type)) = 'execution' OR case_type LIKE '%执行%' THEN 'other'
        ELSE legal_domain
    END,
    domain_source = CASE
        WHEN lower(trim(case_type)) IN ('criminal', 'civil', 'arbitration', 'execution')
             OR case_type LIKE '%刑事%'
             OR case_type LIKE '%民事%'
             OR case_type LIKE '%仲裁%'
             OR case_type LIKE '%执行%' THEN 'legacy'
        ELSE domain_source
    END;

-- 只对强信号做存量推断；刑事画像、“罪”案由或刑事程序关键词优先。
UPDATE cases
SET legal_domain = 'criminal', domain_source = 'inferred'
WHERE legal_domain = 'unknown'
  AND (
      EXISTS (SELECT 1 FROM criminal_case_profiles p WHERE p.case_id = cases.id)
      OR coalesce(cause, '') LIKE '%罪%'
      OR coalesce(agg_cause, '') LIKE '%罪%'
      OR coalesce(name, '') LIKE '%刑事%'
      OR coalesce(name, '') LIKE '%犯罪嫌疑人%'
      OR coalesce(name, '') LIKE '%被告人%'
  );

UPDATE cases
SET legal_domain = 'civil', domain_source = 'inferred'
WHERE legal_domain = 'unknown'
  AND (
      coalesce(cause, '') LIKE '%纠纷%'
      OR coalesce(agg_cause, '') LIKE '%纠纷%'
      OR coalesce(name, '') LIKE '%民事%'
      OR coalesce(name, '') LIKE '%仲裁%'
  );

UPDATE cases
SET legal_domain = 'other', domain_source = 'inferred'
WHERE legal_domain = 'unknown'
  AND (coalesce(name, '') LIKE '%执行%' OR coalesce(cause, '') LIKE '%执行%');
