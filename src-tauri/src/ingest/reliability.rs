//! Small, deterministic guards shared by the ingest pipeline.
//! They deliberately do not invoke OCR: retrying an LLM pass must reuse the cached text.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Domain {
    Civil,
    Criminal,
    Execution,
    Unknown,
}

pub fn classify_domain(existing_type: Option<&str>, text: &str) -> Domain {
    let hint = existing_type.unwrap_or("").trim();
    let normalized_hint = hint.to_ascii_lowercase();
    if hint.contains("刑") || normalized_hint == "criminal" {
        return Domain::Criminal;
    }
    if hint.contains("执行") || normalized_hint == "execution" {
        return Domain::Execution;
    }
    if hint.contains("民")
        || hint.contains("仲裁")
        || matches!(normalized_hint.as_str(), "civil" | "arbitration")
    {
        return Domain::Civil;
    }
    let criminal_markers = [
        "犯罪嫌疑人",
        "被告人",
        "起诉意见书",
        "人民检察院起诉书",
        "刑事判决书",
        "刑事裁定书",
        "逮捕",
        "看守所",
        "认罪认罚",
        "公诉机关",
        "涉嫌犯",
    ];
    if criminal_markers.iter().any(|marker| text.contains(marker))
        || (text.contains("判决书") && (text.contains("刑初") || text.contains("刑终")))
    {
        return Domain::Criminal;
    }
    if text.contains("被执行人") || text.contains("执行案号") {
        return Domain::Execution;
    }
    if text.contains("原告") || text.contains("被告") || text.contains("诉讼请求") {
        return Domain::Civil;
    }
    Domain::Unknown
}

pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 || text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(max_chars)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

pub fn dedupe_trimmed(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let value = value.trim().to_string();
        if !value.is_empty() && !out.iter().any(|x: &String| x == &value) {
            out.push(value);
        }
    }
    out
}

pub fn is_work_record_filename(filename: &str, category: Option<&str>) -> bool {
    let joined = format!("{} {}", filename, category.unwrap_or(""));
    ["工作日志", "会见笔录", "阅卷记录", "庭审记录", "沟通记录"]
        .iter()
        .any(|x| joined.contains(x))
}

pub fn quality_warning(text: &str) -> Option<&'static str> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.chars().count() < 80 {
        return Some("正文过短，需要人工复核");
    }
    let repeated = compact
        .chars()
        .collect::<Vec<_>>()
        .windows(8)
        .any(|w| w.iter().all(|x| *x == w[0]));
    repeated.then_some("文本疑似水印或重复字符干扰，需要人工复核")
}

/// 合并每个长材料分片独立得到的字段。标量保留最早的非空值，数组按 JSON 值去重，
/// 因而不会因后续分片信息较少而覆盖前一片已经识别出的字段。
pub fn merge_extracted_fields(
    fields: impl IntoIterator<Item = crate::llm::ExtractedFields>,
) -> crate::llm::ExtractedFields {
    let mut merged = serde_json::Value::Object(serde_json::Map::new());
    for field in fields {
        merge_json(
            &mut merged,
            serde_json::to_value(field).unwrap_or(serde_json::Value::Null),
        );
    }
    serde_json::from_value(merged).unwrap_or_default()
}

fn merge_json(target: &mut serde_json::Value, incoming: serde_json::Value) {
    match (target, incoming) {
        (serde_json::Value::Object(current), serde_json::Value::Object(next)) => {
            for (key, value) in next {
                merge_json(current.entry(key).or_insert(serde_json::Value::Null), value);
            }
        }
        (serde_json::Value::Array(current), serde_json::Value::Array(next)) => {
            for value in next {
                if !current.contains(&value) {
                    current.push(value);
                }
            }
        }
        (slot @ serde_json::Value::Null, value) if !value.is_null() => *slot = value,
        (serde_json::Value::String(existing), serde_json::Value::String(value))
            if existing.trim().is_empty() && !value.trim().is_empty() =>
        {
            *existing = value;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_does_not_guess() {
        assert_eq!(classify_domain(None, "普通材料"), Domain::Unknown);
    }
    #[test]
    fn english_case_type_routes_criminal_before_civil_keywords() {
        assert_eq!(classify_domain(Some("criminal"), "原告被告"), Domain::Criminal);
    }
    #[test]
    fn chunks_and_dedupes() {
        assert_eq!(chunk_text("甲乙丙丁戊己", 2).concat(), "甲乙丙丁戊己");
        assert_eq!(
            dedupe_trimmed(vec![" 甲 ".into(), "甲".into(), "乙".into()]),
            vec!["甲", "乙"]
        );
    }
    #[test]
    fn merge_chunks_dedupes_arrays_and_retains_first_scalar() {
        let first = crate::llm::ExtractedFields {
            cause: Some("诈骗罪".into()),
            plaintiffs: vec!["甲".into()],
            ..Default::default()
        };
        let second = crate::llm::ExtractedFields {
            cause: Some("后文冲突".into()),
            plaintiffs: vec!["甲".into(), "乙".into()],
            ..Default::default()
        };
        let merged = merge_extracted_fields(vec![first, second]);
        assert_eq!(merged.cause.as_deref(), Some("诈骗罪"));
        assert_eq!(merged.plaintiffs, vec!["甲", "乙"]);
    }
    #[test]
    fn only_work_documents_are_candidates() {
        assert!(is_work_record_filename("会见笔录.docx", None));
        assert!(!is_work_record_filename("民事起诉状.docx", Some("起诉状")));
    }
}
