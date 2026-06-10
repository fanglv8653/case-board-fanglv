//! 查询缓存键算法,跟 Python `~/.claude/skills/yuandian-legal-search/scripts/cache.py`
//! 的 `_query_hash` 100% 对齐(D1.5)。
//!
//! Python 实现:
//!   raw = f"{query_type}:{json.dumps(params, sort_keys=True, ensure_ascii=False)}"
//!   h = hashlib.md5(raw.encode("utf-8")).hexdigest()[:12]
//!   return f"{query_type}-{h}"
//!
//! 跟 Python 一致的 4 个细节(决定了 hash 是否漂移,**改这个文件前请重跑 fixture 测**):
//!   1. **键按字典序排序**(对应 `sort_keys=True`)
//!   2. **中文不转义**(对应 `ensure_ascii=False`)— 直接输出 UTF-8 字节
//!   3. **分隔符是 `, ` 跟 `: `**(Python `json.dumps` 默认值,**有空格**)
//!   4. **嵌套对象递归同规则**
//!
//! Rust 自带的 `serde_json::to_string` 是紧凑模式(无空格),所以这里**不能直接用** —
//! 必须手写一个 emit。

use serde_json::Value;

/// 跟 Python `_query_hash` 完全等价。
pub fn query_hash(query_type: &str, params: &Value) -> String {
    let canonical = canonical_json_str(params);
    let raw = format!("{}:{}", query_type, canonical);
    let digest = md5::compute(raw.as_bytes());
    let hex = format!("{:x}", digest);
    format!("{}-{}", query_type, &hex[..12])
}

/// JSON 规范化:`sort_keys=True, ensure_ascii=False, separators=(', ', ': ')`。
fn canonical_json_str(v: &Value) -> String {
    let mut out = String::new();
    emit(v, &mut out);
    out
}

fn emit(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        // serde_json::Number 的 Display 对整数输出 "10",对浮点输出 "10.0",跟
        // Python `json.dumps` 默认一致(整数无小数点,浮点保留 .0)。
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => emit_string(s, out),
        Value::Array(arr) => {
            out.push('[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            // 键按字典序排序(Python `sort_keys=True`)
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit_string(k, out);
                out.push_str(": ");
                emit(&map[k.as_str()], out);
            }
            out.push('}');
        }
    }
}

/// 字符串 emit:跟 Python `json.dumps(..., ensure_ascii=False)` 严格一致。
/// 转义集合 = `"` `\` `\n` `\r` `\t` `\b` `\f` + 控制字符 `\u00XX`,其他全部直接输出 UTF-8。
fn emit_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                // 其他控制字符:Python 用 \u00XX 小写 hex 输出
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    //! 5 个跨语言 fixture — 标准值由 Python 端实时跑出来(详 § 16.3 的 cheat sheet)。
    //! 跑命令:
    //!   python3 -c "
    //!   import sys, json, hashlib
    //!   sys.path.insert(0, '<path-to>/yuandian-legal-search/scripts')
    //!   from cache import _query_hash
    //!   print(_query_hash('rh_ft_search', {'keyword':'合同解除','top_k':10}))
    //!   ..."
    //!
    //! 任何一个 fixture 挂了 → Rust 写的 KB 缓存 Python skill 端读不到 → blocker。
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_chinese_string_no_escape() {
        let s = canonical_json_str(&json!({"keyword":"合同解除","top_k":10}));
        // 中文不转义,key 排序("keyword" 在 "top_k" 之前),": " 跟 ", " 有空格
        assert_eq!(s, r#"{"keyword": "合同解除", "top_k": 10}"#);
    }

    #[test]
    fn canonical_empty_object() {
        assert_eq!(canonical_json_str(&json!({})), "{}");
    }

    #[test]
    fn canonical_nested_object_sort_keys_recursively() {
        let s = canonical_json_str(&json!({
            "z": 1,
            "a": {"y": 2, "b": null},
        }));
        // 外层 a < z,内层 b < y;null 输出为 "null"
        assert_eq!(s, r#"{"a": {"b": null, "y": 2}, "z": 1}"#);
    }

    #[test]
    fn canonical_array_with_mixed_types() {
        let s = canonical_json_str(&json!([1, "中文", true, null, 3.5]));
        assert_eq!(s, r#"[1, "中文", true, null, 3.5]"#);
    }

    #[test]
    fn canonical_escapes_control_chars_but_not_unicode() {
        let s = canonical_json_str(&json!({"k":"a\nb\t\"c\""}));
        assert_eq!(s, r#"{"k": "a\nb\t\"c\""}"#);
    }

    // ============ 跨语言 fixture(Python 端实时跑出来 2026-05-27)============

    #[test]
    fn fixture_1_ft_search_with_chinese_keyword() {
        let got = query_hash("rh_ft_search", &json!({"keyword":"合同解除","top_k":10}));
        assert_eq!(got, "rh_ft_search-91dc854aae37");
    }

    #[test]
    fn fixture_2_enterprise_search_chinese_name() {
        let got = query_hash("rh_enterpriseSearch", &json!({"name":"无锡XX科技有限公司"}));
        assert_eq!(got, "rh_enterpriseSearch-c3759e68a36c");
    }

    #[test]
    fn fixture_3_hall_detect_long_chinese() {
        let got = query_hash("hall_detect", &json!({"text":"根据《民法典》第563条"}));
        assert_eq!(got, "hall_detect-b4722ae3279f");
    }

    #[test]
    fn fixture_4_case_vector_search_nested_filter() {
        let got = query_hash(
            "case_vector_search",
            &json!({
                "query":"买卖合同 违约金",
                "wenshu_filter":{"ay":"买卖合同纠纷"},
            }),
        );
        assert_eq!(got, "case_vector_search-1c38e636682d");
    }

    #[test]
    fn fixture_5_empty_params() {
        let got = query_hash("search_laws", &json!({}));
        assert_eq!(got, "search_laws-a2ca6205453d");
    }
}
