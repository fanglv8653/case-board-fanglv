use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::SyncError;

#[derive(Debug, Clone, Copy)]
pub struct EntityPolicy {
    pub entity_type: &'static str,
    pub table: &'static str,
    pub case_column: Option<&'static str>,
    pub columns: &'static [&'static str],
    pub atomic_groups: &'static [(&'static str, &'static [&'static str])],
}

const FINANCE_GROUP: &[&str] = &[
    "lawyer_fee_total",
    "share_ratio",
    "firm_deduction_rate",
    "archive_holdback_rate",
    "personal_share_amount",
    "firm_deduction_amount",
    "archive_holdback_amount",
    "archive_returned_amount",
    "actual_income_amount",
    "actual_income_overridden",
];
const FEISHU_BINDING_GROUP: &[&str] = &["app_token", "table_id", "record_id", "slot_key"];
const CASE_DOMAIN_GROUP: &[&str] = &["legal_domain", "domain_source"];

const POLICIES: &[EntityPolicy] = &[
    EntityPolicy {
        entity_type: "case",
        table: "cases",
        case_column: Some("id"),
        columns: &[
            "id",
            "name",
            "case_type",
            "cause",
            "case_no",
            "court",
            "judge_id",
            "stage",
            "created_at",
            "updated_at",
            "agg_case_no",
            "agg_court",
            "agg_cause",
            "agg_plaintiffs",
            "agg_defendants",
            "agg_third_parties",
            "agg_judges",
            "agg_claim_amount",
            "agg_filed_at",
            "next_milestone_type",
            "next_milestone_at",
            "next_milestone_status",
            "next_milestone_note",
            "case_status",
            "execution_total",
            "execution_total_breakdown",
            "execution_started_at",
            "execution_received",
            "execution_remaining",
            "workflow_status",
            "workflow_status_locked",
            "agg_court_type",
            "agg_our_side",
            "legal_domain",
            "domain_source",
            "display_name_override",
            "management_status",
            "management_status_source",
        ],
        atomic_groups: &[("case_domain", CASE_DOMAIN_GROUP)],
    },
    EntityPolicy {
        entity_type: "party",
        table: "parties",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "role",
            "name",
            "party_type",
            "id_no",
            "created_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "contact",
        table: "contacts",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "role",
            "name",
            "phone_office",
            "mobile",
            "wechat",
            "email",
            "notes",
            "created_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "work_item",
        table: "case_work_items",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "occurred_at",
            "work_type",
            "title",
            "content",
            "result",
            "next_action",
            "duration_minutes",
            "source",
            "external_source",
            "external_record_id",
            "external_updated_at",
            "external_status",
            "external_last_seen_at",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "stage_item",
        table: "case_stage_items",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "domain",
            "major_stage",
            "stage_label",
            "status",
            "started_at",
            "due_at",
            "completed_at",
            "reminder_at",
            "source",
            "external_source",
            "external_record_id",
            "external_status",
            "external_updated_at",
            "external_last_seen_at",
            "notes",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "agency_contact",
        table: "case_agency_contacts",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "stage_scope",
            "agency_type",
            "agency_name",
            "contact_role",
            "contact_name",
            "phone",
            "case_no",
            "query_code",
            "notes",
            "source",
            "external_source",
            "external_record_id",
            "external_slot_key",
            "external_updated_at",
            "external_last_seen_at",
            "external_status",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "criminal_deadline",
        table: "criminal_deadline_items",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "stage_item_id",
            "rule_code",
            "title",
            "major_stage",
            "minor_stage",
            "trigger_date",
            "trigger_time",
            "default_due_at",
            "manual_due_at",
            "effective_due_at",
            "reminder_at",
            "priority",
            "status",
            "source_type",
            "source_law",
            "source_article",
            "source_url",
            "calculation_note",
            "exception_type",
            "exception_note",
            "override_reason",
            "completed_at",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "criminal_workflow",
        table: "criminal_case_workflows",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "template_version_id",
            "status",
            "current_stage_code",
            "started_at",
            "closed_at",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "criminal_task",
        table: "criminal_case_tasks",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "workflow_id",
            "case_id",
            "template_node_id",
            "node_code",
            "title",
            "stage_code",
            "stage_sort",
            "node_sort",
            "task_type",
            "applicability_status",
            "status",
            "occurrence_key",
            "occurrence_no",
            "trigger_event",
            "trigger_event_id",
            "trigger_source_type",
            "trigger_source_ref_id",
            "planned_at",
            "original_planned_at",
            "started_at",
            "completed_at",
            "deferred_at",
            "ignored_at",
            "reopened_at",
            "result",
            "next_action",
            "duration_minutes",
            "disposition_reason",
            "client_feedback_recorded",
            "time_nature",
            "deadline_item_id",
            "work_item_id",
            "assigned_to",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "case_todo",
        table: "case_todos",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "title",
            "content",
            "kind",
            "priority",
            "tags_json",
            "next_action",
            "status",
            "done",
            "done_at",
            "due_at",
            "remind_at",
            "due_date",
            "source",
            "source_message_id",
            "source_at",
            "delete_requested_at",
            "delete_reason",
            "deleted_at",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "calendar_event",
        table: "calendar_events",
        case_column: None,
        columns: &["id", "date", "title", "created_at"],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "income_record",
        table: "case_income_records",
        case_column: Some("case_id"),
        columns: &[
            "id",
            "case_id",
            "manual_case_name",
            "lawyer_fee_total",
            "source_type",
            "collaborator_name",
            "share_ratio",
            "firm_deduction_rate",
            "archive_holdback_rate",
            "personal_share_amount",
            "firm_deduction_amount",
            "archive_holdback_amount",
            "archive_holdback_status",
            "archive_returned_at",
            "archive_returned_amount",
            "invoice_date",
            "invoice_no",
            "recognized_month",
            "actual_income_amount",
            "actual_income_overridden",
            "actual_income_override_note",
            "note",
            "record_status",
            "invoice_total",
            "invoice_buyer",
            "invoice_seller",
            "invoice_type",
            "auto_source_document_id",
            "auto_source_filename",
            "auto_fields_json",
            "manual_fields_json",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[("finance_calculation", FINANCE_GROUP)],
    },
    EntityPolicy {
        entity_type: "case_payment",
        table: "case_payments",
        case_column: Some("case_id"),
        columns: &["id", "case_id", "amount", "paid_at", "note", "created_at"],
        atomic_groups: &[("payment", &["amount", "paid_at"])],
    },
    EntityPolicy {
        entity_type: "feishu_link",
        table: "feishu_sync_links",
        case_column: None,
        columns: &[
            "id",
            "entity_type",
            "local_entity_id",
            "app_token",
            "table_id",
            "record_id",
            "slot_key",
            "link_source",
            "status",
            "confirmed_at",
            "last_local_updated_at",
            "last_feishu_modified_at",
            "last_synced_at",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[("feishu_binding", FEISHU_BINDING_GROUP)],
    },
    EntityPolicy {
        entity_type: "feishu_snapshot",
        table: "feishu_sync_snapshots",
        case_column: None,
        columns: &[
            "id",
            "link_id",
            "local_updated_at",
            "feishu_modified_at",
            "payload_hash",
            "mapped_payload_json",
            "created_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "feishu_conflict",
        table: "feishu_sync_conflicts",
        case_column: None,
        columns: &[
            "id",
            "link_id",
            "field_key",
            "base_value_json",
            "local_value_json",
            "feishu_value_json",
            "status",
            "resolution_value_json",
            "resolved_by",
            "resolved_at",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "feishu_inbox",
        table: "feishu_sync_inbox",
        case_column: Some("bound_case_id"),
        columns: &[
            "id",
            "app_token",
            "table_id",
            "record_id",
            "display_name",
            "legal_type",
            "case_no",
            "remote_modified_at",
            "status",
            "bound_case_id",
            "resolved_at",
            "auto_bind_suppressed",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[("feishu_binding", &["app_token", "table_id", "record_id"])],
    },
    EntityPolicy {
        entity_type: "feishu_binding_audit",
        table: "feishu_sync_binding_audits",
        case_column: Some("next_case_id"),
        columns: &[
            "id",
            "inbox_id",
            "action",
            "previous_status",
            "next_status",
            "previous_case_id",
            "next_case_id",
            "created_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "legal_skill_package",
        table: "legal_skill_packages",
        case_column: None,
        columns: &[
            "id",
            "slug",
            "title",
            "version",
            "description",
            "origin",
            "status",
            "manifest_json",
            "package_content_json",
            "content_hash",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[(
            "legal_skill_package",
            &[
                "slug",
                "version",
                "manifest_json",
                "package_content_json",
                "content_hash",
            ],
        )],
    },
    EntityPolicy {
        entity_type: "legal_skill_binding",
        table: "legal_skill_bindings",
        case_column: None,
        columns: &[
            "id",
            "skill_id",
            "legal_domain",
            "task_type",
            "is_default",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[],
    },
    EntityPolicy {
        entity_type: "legal_skill_binding_suppression",
        table: "legal_skill_binding_suppressions",
        case_column: None,
        columns: &[
            "id",
            "legal_domain",
            "task_type",
            "reason",
            "created_at",
            "updated_at",
        ],
        atomic_groups: &[(
            "legal_skill_binding_suppression",
            &["legal_domain", "task_type", "reason"],
        )],
    },
];

pub fn policy(entity_type: &str) -> Result<&'static EntityPolicy, SyncError> {
    POLICIES
        .iter()
        .find(|item| item.entity_type == entity_type)
        .ok_or_else(|| SyncError::EntityNotAllowed(entity_type.to_string()))
}

pub fn all_policies() -> &'static [EntityPolicy] {
    POLICIES
}

pub fn sanitize_fields(
    entity_type: &str,
    fields: &Map<String, Value>,
) -> Result<BTreeMap<String, Value>, SyncError> {
    const EXPLICITLY_DENIED: &[&str] = &[
        "raw_payload_json",
        "source_path",
        "source_folder",
        "extracted_text",
        "chat_messages",
        "memory",
        concat!("access_", "token"),
        concat!("refresh_", "token"),
        concat!("client_", "secret"),
        "password",
    ];
    let policy = policy(entity_type)?;
    let allowed: BTreeSet<&str> = policy.columns.iter().copied().collect();
    let mut clean = BTreeMap::new();
    for (key, value) in fields {
        // The allow-list below is the primary boundary. Keep this explicit
        // deny-list for sensitive fields that must remain rejected even if a
        // future policy accidentally lists one; substring matching is unsafe
        // because legitimate fields include `app_token` and `wechat`.
        if EXPLICITLY_DENIED.contains(&key.as_str()) {
            return Err(SyncError::FieldNotAllowed {
                entity_type: entity_type.to_string(),
                field: key.clone(),
            });
        }
        if !allowed.contains(key.as_str()) {
            return Err(SyncError::FieldNotAllowed {
                entity_type: entity_type.to_string(),
                field: key.clone(),
            });
        }
        clean.insert(key.clone(), value.clone());
    }
    Ok(clean)
}

pub fn atomic_group_for_field(entity_type: &str, field: &str) -> Option<&'static str> {
    policy(entity_type).ok().and_then(|policy| {
        policy
            .atomic_groups
            .iter()
            .find(|(_, fields)| fields.contains(&field))
            .map(|(name, _)| *name)
    })
}

pub fn atomic_group_fields(
    entity_type: &str,
    group: &str,
) -> Result<&'static [&'static str], SyncError> {
    policy(entity_type)?
        .atomic_groups
        .iter()
        .find(|(name, _)| *name == group)
        .map(|(_, fields)| *fields)
        .ok_or_else(|| SyncError::Protocol(format!("未知原子字段组: {entity_type}/{group}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_material_chat_memory_and_raw_feishu_payloads() {
        for (entity, field) in [
            ("case", "source_folder"),
            ("work_item", "raw_payload_json"),
            ("case", "chat_messages"),
            ("case", "memory"),
        ] {
            let fields = json!({ field: "sensitive" }).as_object().unwrap().clone();
            assert!(sanitize_fields(entity, &fields).is_err());
        }
    }

    #[test]
    fn recognizes_finance_and_binding_atomic_groups() {
        assert_eq!(
            atomic_group_for_field("income_record", "share_ratio"),
            Some("finance_calculation")
        );
        assert_eq!(
            atomic_group_for_field("feishu_link", "record_id"),
            Some("feishu_binding")
        );
        let contact = json!({"wechat": "wx-user"}).as_object().unwrap().clone();
        assert!(sanitize_fields("contact", &contact).is_ok());
        let link = json!({"app_token": "app-token"})
            .as_object()
            .unwrap()
            .clone();
        assert!(sanitize_fields("feishu_link", &link).is_ok());
    }
}
