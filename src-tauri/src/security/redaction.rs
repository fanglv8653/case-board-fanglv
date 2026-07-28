//! Central best-effort redaction for third-party errors and diagnostics.

use regex::Regex;

const REDACTED: &str = "[REDACTED]";

pub fn redact(input: &str) -> String {
    let mut output = input.to_string();
    let patterns = [
        r#"(?i)(authorization\s*:\s*(?:bearer|basic)\s+)[^\s,;"']+"#,
        r#"(?i)((?:x-api-key|api[_-]?key|access[_-]?token|refresh[_-]?token|password|secret)["']?\s*[:=]\s*["']?)[^"'\s,;&}]+"#,
        r#"(?i)([?&](?:api[_-]?key|token|access[_-]?token|password|secret)=)[^&#\s]+"#,
        r#"(?i)(https?://)[^/@\s:]+:[^/@\s]+@"#,
        r#"(?i)((?:--password|--token|--api-key|--secret)(?:=|\s+))["']?[^\s"']+"#,
    ];
    for pattern in patterns {
        if let Ok(regex) = Regex::new(pattern) {
            output = regex
                .replace_all(&output, |caps: &regex::Captures<'_>| {
                    if caps.len() > 1 {
                        format!("{}{REDACTED}", &caps[1])
                    } else {
                        REDACTED.to_string()
                    }
                })
                .into_owned();
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_headers_json_urls_and_cli_flags() {
        let markers = [
            "bearer-marker",
            "json-marker",
            "query-marker",
            "userinfo-marker",
            "cli-marker",
        ];
        let text = format!(
            "Authorization: Bearer {}; {{\"api_key\":\"{}\"}} \
             https://example.test?a=1&token={} https://user:{}@example.test \
             --password {}",
            markers[0], markers[1], markers[2], markers[3], markers[4]
        );
        let safe = redact(&text);
        for marker in markers {
            assert!(!safe.contains(marker), "marker leaked: {marker}");
        }
        assert!(safe.contains(REDACTED));
    }
}
