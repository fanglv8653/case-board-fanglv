-- 刑辩五区工作台：只追加结构，不触碰原始材料及既有画像、期限、SOP、任务。
CREATE TABLE criminal_review_notes (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, document_id TEXT, title TEXT NOT NULL, content TEXT NOT NULL,
 note_type TEXT NOT NULL DEFAULT 'general' CHECK(note_type IN ('general','fact','question','contradiction','todo')),
 review_status TEXT NOT NULL DEFAULT 'draft' CHECK(review_status IN ('draft','pending_review','confirmed','rejected')),
 author_type TEXT NOT NULL DEFAULT 'user' CHECK(author_type IN ('user','native_ai','codex')),
 reviewed_by TEXT, reviewed_at TEXT, review_note TEXT, revision INTEGER NOT NULL DEFAULT 1,
 deleted_at TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE, FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE SET NULL
);
CREATE INDEX idx_criminal_review_notes_case ON criminal_review_notes(case_id,review_status,updated_at DESC);

CREATE TABLE criminal_evidence_items (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, name TEXT NOT NULL, evidence_type TEXT NOT NULL DEFAULT 'other',
 proof_purpose TEXT NOT NULL DEFAULT '', source_description TEXT NOT NULL DEFAULT '', originality_status TEXT NOT NULL DEFAULT 'not_reviewed',
 authenticity_assessment_json TEXT NOT NULL DEFAULT '{}', legality_assessment_json TEXT NOT NULL DEFAULT '{}', relevance_assessment_json TEXT NOT NULL DEFAULT '{}',
 admissibility_assessment_json TEXT NOT NULL DEFAULT '{}', probative_force_assessment_json TEXT NOT NULL DEFAULT '{}',
 corroboration_assessment_json TEXT NOT NULL DEFAULT '{}', exclusion_clue_assessment_json TEXT NOT NULL DEFAULT '{}', reasonable_doubt_impact_json TEXT NOT NULL DEFAULT '{}',
 review_status TEXT NOT NULL DEFAULT 'pending_review' CHECK(review_status IN ('pending_review','confirmed','rejected')),
 origin TEXT NOT NULL DEFAULT 'user' CHECK(origin IN ('user','native_ai','codex')), reviewed_by TEXT, reviewed_at TEXT, review_note TEXT,
 revision INTEGER NOT NULL DEFAULT 1, deleted_at TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_criminal_evidence_case ON criminal_evidence_items(case_id,review_status,updated_at DESC);

CREATE TABLE criminal_issues (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL,
 issue_type TEXT NOT NULL CHECK(issue_type IN ('fact','element','procedure','evidence_conflict','evidence_gap','sentencing','other')),
 neutral_title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','confirmed','resolved','archived')),
 position TEXT NOT NULL DEFAULT 'neutral' CHECK(position IN ('prosecution','defense','neutral')),
 origin TEXT NOT NULL DEFAULT 'user' CHECK(origin IN ('user','native_ai','codex')),
 review_status TEXT NOT NULL DEFAULT 'pending_review' CHECK(review_status IN ('pending_review','confirmed','rejected')),
 reviewed_by TEXT, reviewed_at TEXT, review_note TEXT, revision INTEGER NOT NULL DEFAULT 1, deleted_at TEXT,
 created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')), FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_criminal_issues_case ON criminal_issues(case_id,review_status,status);

CREATE TABLE criminal_issue_evidence_links (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, issue_id TEXT NOT NULL, evidence_id TEXT,
 relation TEXT NOT NULL CHECK(relation IN ('supports','contradicts','weakens','contextual','gap')), explanation TEXT NOT NULL DEFAULT '',
 origin TEXT NOT NULL DEFAULT 'user' CHECK(origin IN ('user','native_ai','codex')),
 review_status TEXT NOT NULL DEFAULT 'pending_review' CHECK(review_status IN ('pending_review','confirmed','rejected')),
 reviewed_by TEXT, reviewed_at TEXT, review_note TEXT, revision INTEGER NOT NULL DEFAULT 1, deleted_at TEXT,
 created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE, FOREIGN KEY(issue_id) REFERENCES criminal_issues(id) ON DELETE CASCADE,
 FOREIGN KEY(evidence_id) REFERENCES criminal_evidence_items(id) ON DELETE SET NULL,
 CHECK((relation='gap' AND evidence_id IS NULL) OR (relation<>'gap' AND evidence_id IS NOT NULL))
);
CREATE UNIQUE INDEX uq_criminal_issue_evidence_link ON criminal_issue_evidence_links(issue_id,ifnull(evidence_id,''),relation) WHERE deleted_at IS NULL;

CREATE TABLE criminal_analysis_runs (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, template_code TEXT NOT NULL, template_version INTEGER NOT NULL DEFAULT 1,
 requested_provider TEXT NOT NULL CHECK(requested_provider IN ('manual','native_llm','codex')),
 actual_provider TEXT NOT NULL CHECK(actual_provider IN ('manual_template','native_llm','codex')),
 status TEXT NOT NULL CHECK(status IN ('queued','running','succeeded','partial','failed','cancelled')), request_id TEXT NOT NULL UNIQUE,
 input_snapshot_json TEXT NOT NULL DEFAULT '{}', fallback_from TEXT, fallback_to TEXT, fallback_reason TEXT, error_code TEXT, error_message TEXT,
 started_at TEXT, completed_at TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_criminal_analysis_runs_case ON criminal_analysis_runs(case_id,created_at DESC);

CREATE TABLE criminal_analysis_findings (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, run_id TEXT,
 finding_type TEXT NOT NULL CHECK(finding_type IN ('material_fact','unverified_fact','legal_rule','analysis','defense_strategy')),
 title TEXT NOT NULL, content TEXT NOT NULL, confidence REAL,
 review_status TEXT NOT NULL DEFAULT 'pending_review' CHECK(review_status IN ('pending_review','confirmed','rejected','superseded')),
 origin TEXT NOT NULL DEFAULT 'user' CHECK(origin IN ('user','native_ai','codex')), reviewed_by TEXT, reviewed_at TEXT, review_note TEXT,
 revision INTEGER NOT NULL DEFAULT 1, deleted_at TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE, FOREIGN KEY(run_id) REFERENCES criminal_analysis_runs(id) ON DELETE SET NULL
);
CREATE INDEX idx_criminal_findings_case ON criminal_analysis_findings(case_id,review_status,finding_type);

CREATE TABLE criminal_draft_documents (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, document_type TEXT NOT NULL, title TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived')), current_version_id TEXT, created_by TEXT NOT NULL,
 revision INTEGER NOT NULL DEFAULT 1, deleted_at TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE TABLE criminal_draft_versions (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, draft_id TEXT NOT NULL, version_no INTEGER NOT NULL, content_json TEXT NOT NULL DEFAULT '{}', rendered_markdown TEXT NOT NULL DEFAULT '',
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','pending_review','approved','superseded')),
 origin TEXT NOT NULL DEFAULT 'user' CHECK(origin IN ('user','native_ai','codex')), source_snapshot_json TEXT NOT NULL DEFAULT '{}', quality_report_json TEXT NOT NULL DEFAULT '{}',
 reviewed_by TEXT, reviewed_at TEXT, review_note TEXT, approved_at TEXT, revision INTEGER NOT NULL DEFAULT 1,
 created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE, FOREIGN KEY(draft_id) REFERENCES criminal_draft_documents(id) ON DELETE CASCADE,
 UNIQUE(draft_id,version_no)
);
CREATE INDEX idx_criminal_draft_case ON criminal_draft_documents(case_id,status,updated_at DESC);

CREATE TABLE criminal_source_citations (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL,
 owner_type TEXT NOT NULL CHECK(owner_type IN ('review_note','evidence','issue_link','finding','draft_version')), owner_id TEXT NOT NULL,
 citation_kind TEXT NOT NULL CHECK(citation_kind IN ('material','legal','user_statement')), document_id TEXT,
 source_filename_snapshot TEXT, source_path_snapshot TEXT, source_fingerprint TEXT, page_start INTEGER, page_end INTEGER, locator_json TEXT NOT NULL DEFAULT '{}',
 location_precision TEXT NOT NULL DEFAULT 'exact' CHECK(location_precision IN ('exact','approximate')), excerpt TEXT NOT NULL DEFAULT '',
 legal_title TEXT, legal_article TEXT, legal_url TEXT, verification_status TEXT NOT NULL DEFAULT 'unchecked' CHECK(verification_status IN ('unchecked','verified','cannot_verify')),
 integrity_status TEXT NOT NULL DEFAULT 'valid' CHECK(integrity_status IN ('valid','missing','changed')), checked_at TEXT,
 revision INTEGER NOT NULL DEFAULT 1, deleted_at TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE, FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE SET NULL
);
CREATE INDEX idx_criminal_citations_owner ON criminal_source_citations(owner_type,owner_id,deleted_at);
CREATE INDEX idx_criminal_citations_integrity ON criminal_source_citations(case_id,integrity_status);

CREATE TABLE criminal_workspace_task_links (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, task_id TEXT NOT NULL,
 artifact_type TEXT NOT NULL CHECK(artifact_type IN ('review_note','evidence','issue','finding','draft_version')), artifact_id TEXT NOT NULL,
 relation TEXT NOT NULL CHECK(relation IN ('input','output','supports','follow_up')), created_by TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE, FOREIGN KEY(task_id) REFERENCES criminal_case_tasks(id) ON DELETE CASCADE,
 UNIQUE(task_id,artifact_type,artifact_id,relation)
);
CREATE TABLE criminal_workspace_audit_events (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id TEXT NOT NULL, event_type TEXT NOT NULL,
 actor TEXT NOT NULL, from_status TEXT, to_status TEXT, payload_json TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL DEFAULT(datetime('now')),
 FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_criminal_workspace_audit ON criminal_workspace_audit_events(case_id,aggregate_type,aggregate_id,created_at DESC);

-- 审计与业务写同一 SQLite 语句/事务提交，任何审计写失败都会使业务写回滚。
CREATE TRIGGER audit_review_note_insert AFTER INSERT ON criminal_review_notes BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'review_note',NEW.id,'created',NEW.author_type,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_review_note_update AFTER UPDATE ON criminal_review_notes BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'review_note',NEW.id,CASE WHEN NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN 'deleted' WHEN NEW.review_status='confirmed' THEN 'review_confirmed' WHEN NEW.review_status='rejected' THEN 'review_rejected' WHEN NEW.review_status='pending_review' AND OLD.review_status<>NEW.review_status THEN 'review_reopened' ELSE 'updated' END,coalesce(NEW.reviewed_by,NEW.author_type),OLD.review_status,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_evidence_insert AFTER INSERT ON criminal_evidence_items BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'evidence',NEW.id,'created',NEW.origin,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_evidence_update AFTER UPDATE ON criminal_evidence_items BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'evidence',NEW.id,CASE WHEN NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN 'deleted' WHEN NEW.review_status='confirmed' THEN 'review_confirmed' WHEN NEW.review_status='rejected' THEN 'review_rejected' WHEN NEW.review_status='pending_review' AND OLD.review_status<>NEW.review_status THEN 'review_reopened' ELSE 'updated' END,coalesce(NEW.reviewed_by,NEW.origin),OLD.review_status,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_issue_insert AFTER INSERT ON criminal_issues BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'issue',NEW.id,'created',NEW.origin,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_issue_update AFTER UPDATE ON criminal_issues BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'issue',NEW.id,CASE WHEN NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN 'deleted' WHEN NEW.review_status='confirmed' THEN 'review_confirmed' WHEN NEW.review_status='rejected' THEN 'review_rejected' WHEN NEW.review_status='pending_review' AND OLD.review_status<>NEW.review_status THEN 'review_reopened' ELSE 'updated' END,coalesce(NEW.reviewed_by,NEW.origin),OLD.review_status,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_issue_link_insert AFTER INSERT ON criminal_issue_evidence_links BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'issue_link',NEW.id,'created',NEW.origin,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_issue_link_update AFTER UPDATE ON criminal_issue_evidence_links BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'issue_link',NEW.id,CASE WHEN NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN 'deleted' WHEN NEW.review_status='confirmed' THEN 'review_confirmed' WHEN NEW.review_status='rejected' THEN 'review_rejected' WHEN NEW.review_status='pending_review' AND OLD.review_status<>NEW.review_status THEN 'review_reopened' ELSE 'updated' END,coalesce(NEW.reviewed_by,NEW.origin),OLD.review_status,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_finding_insert AFTER INSERT ON criminal_analysis_findings BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'finding',NEW.id,'created',NEW.origin,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_finding_update AFTER UPDATE ON criminal_analysis_findings BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'finding',NEW.id,CASE WHEN NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN 'deleted' WHEN NEW.review_status='confirmed' THEN 'review_confirmed' WHEN NEW.review_status='rejected' THEN 'review_rejected' WHEN NEW.review_status='pending_review' AND OLD.review_status<>NEW.review_status THEN 'review_reopened' ELSE 'updated' END,coalesce(NEW.reviewed_by,NEW.origin),OLD.review_status,NEW.review_status,'{}'); END;
CREATE TRIGGER audit_draft_insert AFTER INSERT ON criminal_draft_documents BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'draft',NEW.id,'created',NEW.created_by,NEW.status,'{}'); END;
CREATE TRIGGER audit_draft_update AFTER UPDATE ON criminal_draft_documents BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'draft',NEW.id,'updated',NEW.created_by,OLD.status,NEW.status,'{}'); END;
CREATE TRIGGER audit_draft_version_insert AFTER INSERT ON criminal_draft_versions BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'draft_version',NEW.id,'created',NEW.origin,NEW.status,'{}'); END;
CREATE TRIGGER audit_draft_version_update AFTER UPDATE ON criminal_draft_versions BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'draft_version',NEW.id,CASE WHEN NEW.status='approved' THEN 'approved' WHEN NEW.status='superseded' THEN 'superseded' WHEN NEW.status='pending_review' THEN 'submitted_for_review' WHEN NEW.status='draft' AND OLD.status='pending_review' THEN 'review_returned' ELSE 'updated' END,coalesce(NEW.reviewed_by,NEW.origin),OLD.status,NEW.status,'{}'); END;
CREATE TRIGGER audit_citation_insert AFTER INSERT ON criminal_source_citations BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'citation',NEW.id,'created','user',NEW.integrity_status,'{}'); END;
CREATE TRIGGER audit_citation_update AFTER UPDATE ON criminal_source_citations BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'citation',NEW.id,CASE WHEN NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN 'deleted' WHEN NEW.integrity_status<>OLD.integrity_status THEN 'integrity_changed' ELSE 'updated' END,'user',OLD.integrity_status,NEW.integrity_status,'{}'); END;
CREATE TRIGGER audit_task_link_insert AFTER INSERT ON criminal_workspace_task_links BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'task_link',NEW.id,'created',NEW.created_by,'{}'); END;
CREATE TRIGGER audit_task_link_delete AFTER DELETE ON criminal_workspace_task_links BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,payload_json)
 VALUES(lower(hex(randomblob(16))),OLD.case_id,'task_link',OLD.id,'deleted',OLD.created_by,'{}'); END;
CREATE TRIGGER audit_analysis_run_insert AFTER INSERT ON criminal_analysis_runs BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'analysis_run',NEW.id,'created',NEW.actual_provider,NEW.status,'{}'); END;
CREATE TRIGGER audit_analysis_run_update AFTER UPDATE ON criminal_analysis_runs BEGIN
 INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,from_status,to_status,payload_json)
 VALUES(lower(hex(randomblob(16))),NEW.case_id,'analysis_run',NEW.id,'status_changed',NEW.actual_provider,OLD.status,NEW.status,'{}'); END;
