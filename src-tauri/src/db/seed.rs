//! 2026-05-25 V0.1.6 · 示例案件 seed。
//!
//! 在 onboarding 完成时调用一次:如果 cases 表空,自动 INSERT 一个虚构的
//! 「张三 诉 李四 民间借贷纠纷」全流程示例,让用户首次进 App 不是空白页。
//!
//! 数据特征:
//!   - `source_folder = '__DEMO__'`,前端识别后禁用「打开源文件夹」「刷新源文件」
//!   - 案件涵盖完整流程:立案 → 调解 → 调解书生效 → 申请执行 → 执行立案 → 部分回款
//!   - 用户可正常点击查看 / 删除(走标准 delete_case 逻辑)
//!   - 落盘一份「案件分析报告」到 reports/<demo_id>.md,详情页「📖 案件报告」可看
//!
//! 隐私:**纯虚构姓名 / 金额 / 案号**,跟任何真实案件无关。

use sqlx::SqlitePool;

const DEMO_CASE_ID: &str = "demo-zhang-li-loan-2024";
const DEMO_SOURCE_FOLDER: &str = "__DEMO__";

/// 案件分析报告 MD 内容(虚构示例,展示 LLM 全局抽的成果)。
const DEMO_REPORT_MD: &str = r#"# 案件分析报告 · 张三 诉 李四 民间借贷纠纷

> 📌 这是一个**示例案件**,用于演示 CaseBoard 的功能。
> 案号、当事人、金额、法院全部虚构,跟任何真实案件无关。
> 你可以点右上角 🗑 删除按钮把它清掉,然后导入自己的真实案件。

---

## 一、案件概况

- **案号**:(2024)苏0211民初9999号(虚构)
- **承办法院**:无锡市惠山区人民法院
- **案由**:民间借贷纠纷
- **立案日期**:2024-04-15
- **标的金额**:人民币 50 万元
- **当前阶段**:执行中(已部分回款 15 万元)
- **调解结果**:双方达成调解,被告应于 2024-09-30 前一次性返还借款本金 50 万元及利息 25,000 元

## 二、当事人

| 角色 | 姓名 | 联系方式 | 我方 |
|:---|:---|:---|:---:|
| 原告 | 张三 | 138****0001 | ✓ |
| 被告 | 李四 | 139****0002 | — |

## 三、关键时间线

| 日期 | 事件 |
|:---|:---|
| 2024-03-10 | 借款交付(银行转账 50 万元) |
| 2024-04-15 | 立案 · 无锡惠山法院 |
| 2024-04-25 | 财产保全裁定 · 冻结被告建行账户 60 万元 |
| 2024-05-20 | 庭审(调解庭) |
| 2024-06-20 | 调解书生效 |
| 2024-09-30 | 履行期届满(被告未履行) |
| 2024-10-15 | 申请执行 |
| 2024-10-20 | 执行立案 (2024)苏0211执9999号 |
| 2024-11-05 | 财产线索查询(发现工资账户) |
| 2025-01-15 | 部分到账 10 万元(被告主动转账) |
| 2025-03-20 | 部分到账 5 万元(强制执行 · 工资扣划) |

## 四、收费记录

| 项目 | 金额(元) |
|:---|---:|
| 案件受理费 | 4,400 |
| 财产保全费 | 2,520 |
| 律师代理费 | 20,000 |
| **合计** | **26,920** |

## 五、执行追踪

- **执行金额**:525,000 元(本金 50 万 + 利息 2.5 万)
- **已收回**:150,000 元(2 笔)
- **剩余**:375,000 元

## 六、风险提示与下一步建议

1. **保全到期跟进** — 资金保全 2024-04-25 生效,期限 1 年,**已于 2025-04-25 到期**
   - 已申请续封?需确认续封裁定是否下发
2. **被执行人财产线索**
   - 工资账户已扣划,可继续按月扣
   - 建议查询是否有不动产 / 车辆 / 股权(可点详情页「🔍 查被执行人」)
3. **利息计算**
   - 自履行期届满日 2024-09-30 起,按日万分之五计算违约金
   - 截至 2025-05-25,违约金约 5 万元(可用「利息执行款」工具精确计算)

---

*本报告由 LLM 全局抽取 + 模板生成。这是虚构示例,实际案件请用「更新」按钮重新抽取。*
"#;

/// 如果 cases 表为空,seed 一个示例案件。已有案件就跳过。
///
/// 返回:Ok(true) = 真的 seed 了;Ok(false) = 表非空,跳过。
pub async fn seed_demo_case_if_empty(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    // 检查 cases 是否为空
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cases")
        .fetch_one(pool)
        .await?;
    if count.0 > 0 {
        return Ok(false);
    }

    // 1) 落盘报告 MD
    let report_path: Option<String> =
        match crate::llm::global_extract::report_path_for_case(DEMO_CASE_ID) {
            Ok(p) => {
                if std::fs::write(&p, DEMO_REPORT_MD).is_ok() {
                    Some(p.to_string_lossy().to_string())
                } else {
                    None
                }
            }
            Err(_) => None,
        };

    // 2) JSON 字段
    let plaintiffs_json = r#"["张三"]"#;
    let defendants_json = r#"["李四"]"#;
    let judges_json = r#"["王法官"]"#;

    let party_contacts_json = r#"[
        {"party":"张三","name":"张三","role":"本人","phone":"138****0001","email":null,"is_our_side":true},
        {"party":"李四","name":"李四","role":"本人","phone":"139****0002","email":null,"is_our_side":false}
    ]"#;

    let court_contacts_json = r#"[
        {"name":"王法官","role":"主办法官","phone":"0510-1234-5678"},
        {"name":"李书记员","role":"书记员","phone":"0510-1234-5679"}
    ]"#;

    let key_dates_json = r#"[
        {"event_type":"借款交付","date":"2024-03-10","note":"银行转账 50 万元"},
        {"event_type":"立案","date":"2024-04-15","note":"无锡市惠山区人民法院"},
        {"event_type":"财产保全","date":"2024-04-25","note":"冻结被告建行账户 60 万元","expires_at":"2025-04-25"},
        {"event_type":"开庭","date":"2024-05-20","note":"调解庭"},
        {"event_type":"调解书生效","date":"2024-06-20","note":"(2024)苏0211民初9999号"},
        {"event_type":"履行期届满","date":"2024-09-30","note":"被告未履行","expires_at":"2024-09-30"},
        {"event_type":"申请执行","date":"2024-10-15","note":null},
        {"event_type":"执行立案","date":"2024-10-20","note":"(2024)苏0211执9999号"},
        {"event_type":"财产线索查询","date":"2024-11-05","note":"发现工资账户"}
    ]"#;

    let fees_json = r#"[
        {"item":"案件受理费","amount":4400,"charged_at":"2024-04-15","receipt_no":null,"note":null},
        {"item":"财产保全费","amount":2520,"charged_at":"2024-04-25","receipt_no":null,"note":null},
        {"item":"律师代理费","amount":20000,"charged_at":"2024-04-10","receipt_no":null,"note":"前期 50%"}
    ]"#;

    let resolution = "经法院主持调解,双方于 2024-06-20 达成调解协议:被告李四应于 2024-09-30 前一次性向原告张三返还借款本金 50 万元及利息 25,000 元。逾期按日万分之五计算违约金。调解书 (2024)苏0211民初9999号 已生效。";
    let status_text = "调解书已生效进入执行,被告部分履行 15 万元,剩余 37.5 万元继续追索中。资金保全已于 2025-04-25 到期需关注续封。";
    let summary = "张三诉李四民间借贷 50 万元,经调解书生效,已进入执行,部分回款 15 万元。";

    let now = chrono::Utc::now().to_rfc3339();

    // 3) INSERT cases
    sqlx::query(
        r#"INSERT INTO cases (
            id, name, case_type, cause, case_no, court, source_folder,
            workflow_status, case_status,
            agg_case_no, agg_court, agg_cause, agg_filed_at, agg_claim_amount,
            agg_plaintiffs, agg_defendants, agg_third_parties, agg_judges,
            agg_party_contacts, agg_court_contacts, agg_key_dates, agg_fees,
            agg_resolution, agg_status_text, case_summary,
            case_report_path, case_report_generated_at,
            agg_computed_at, created_at, updated_at
        ) VALUES (
            ?, '张三 诉 李四 民间借贷纠纷', '诉讼', '民间借贷纠纷',
            '(2024)苏0211民初9999号', '无锡市惠山区人民法院', ?,
            'execution', '进行中',
            '(2024)苏0211民初9999号', '无锡市惠山区人民法院', '民间借贷纠纷',
            '2024-04-15', 500000.0,
            ?, ?, '[]', ?,
            ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?,
            ?, ?, ?
        )"#,
    )
    .bind(DEMO_CASE_ID)
    .bind(DEMO_SOURCE_FOLDER)
    .bind(plaintiffs_json)
    .bind(defendants_json)
    .bind(judges_json)
    .bind(party_contacts_json)
    .bind(court_contacts_json)
    .bind(key_dates_json)
    .bind(fees_json)
    .bind(resolution)
    .bind(status_text)
    .bind(summary)
    .bind(report_path.as_deref())
    .bind(
        report_path
            .as_deref()
            .map(|_| now.clone())
            .unwrap_or_default(),
    )
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    // 4) parties 子表
    sqlx::query(
        "INSERT INTO parties (id, case_id, role, name, party_type, contact_phone) VALUES \
            (?, ?, '原告', '张三', '自然人', '138****0001'), \
            (?, ?, '被告', '李四', '自然人', '139****0002')",
    )
    .bind(format!("{}-p1", DEMO_CASE_ID))
    .bind(DEMO_CASE_ID)
    .bind(format!("{}-p2", DEMO_CASE_ID))
    .bind(DEMO_CASE_ID)
    .execute(pool)
    .await?;

    // 5) contacts 子表(法院联系人)
    sqlx::query(
        "INSERT INTO contacts (id, case_id, role, name, phone_office) VALUES \
            (?, ?, '法官', '王法官', '0510-1234-5678'), \
            (?, ?, '书记员', '李书记员', '0510-1234-5679')",
    )
    .bind(format!("{}-c1", DEMO_CASE_ID))
    .bind(DEMO_CASE_ID)
    .bind(format!("{}-c2", DEMO_CASE_ID))
    .bind(DEMO_CASE_ID)
    .execute(pool)
    .await?;

    // 6) case_payments(已收回 2 笔)
    sqlx::query(
        "INSERT INTO case_payments (id, case_id, amount, paid_at, note) VALUES \
            (?, ?, 100000.0, '2025-01-15', '银行转账 / 被告主动履行'), \
            (?, ?, 50000.0, '2025-03-20', '强制执行 / 工资扣划')",
    )
    .bind(format!("{}-pay1", DEMO_CASE_ID))
    .bind(DEMO_CASE_ID)
    .bind(format!("{}-pay2", DEMO_CASE_ID))
    .bind(DEMO_CASE_ID)
    .execute(pool)
    .await?;

    // 7) case_preservations(资金保全 · 已到期)
    sqlx::query(
        r#"INSERT INTO case_preservations (
            id, case_id, target_type, target_detail, preserved_amount,
            court, doc_no, started_at, duration_years, expires_at, status
        ) VALUES (
            ?, ?, '账户冻结', '被告李四在中国建设银行无锡分行账户', 600000.0,
            '无锡市惠山区人民法院', '(2024)苏0211民初9999号财保',
            '2024-04-25', 1, '2025-04-25', 'expired'
        )"#,
    )
    .bind(format!("{}-pres1", DEMO_CASE_ID))
    .bind(DEMO_CASE_ID)
    .execute(pool)
    .await?;

    Ok(true)
}
