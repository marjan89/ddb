//! Shared parser for the semantic-agent /semantic YAML response.
//!
//! Single source of truth for both `cmd::crawl` (structured fields) and
//! `cmd::test_element` (raw chunks for substring matching). This module
//! preserves the exact behavior of the previous crawl + test parsers
//! (bug-for-bug parity); structural improvements are tracked separately.

#[derive(Debug, Clone)]
pub struct ElementRecord {
    /// Raw YAML chunk (for substring search by the test runner).
    pub raw: String,
    pub content: String,
    pub etype: String,
    /// Matches either `id` or `platform_id` from the agent dump.
    pub id: Option<String>,
    pub bounds: Option<[i32; 4]>,
    pub clickable: bool,
}

/// Split the agent YAML into raw chunks delimited by top-level list items
/// (`\n- ` marker). Preserves the legacy format consumed by the test runner's
/// substring searches.
pub fn split_elements(yaml: &str) -> Vec<String> {
    yaml.split("\n- ").map(|s| s.to_string()).collect()
}

fn field_key(line: &str) -> &str {
    line.split_once(':')
        .map(|(k, _)| k)
        .unwrap_or("")
        .trim_start_matches('-')
        .trim()
}

fn extract_value(line: &str) -> String {
    line.split_once(':')
        .map(|(_, v)| v)
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn parse_bounds(s: &str) -> Option<[i32; 4]> {
    let inner = s.trim_start_matches('[').trim_end_matches(']');
    let parts: Vec<i32> = inner.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() == 4 { Some([parts[0], parts[1], parts[2], parts[3]]) } else { None }
}

/// Parse all elements from the agent YAML body. Field-order-agnostic;
/// matches the current crawl behavior including the `id`/`platform_id` alias.
pub fn parse_elements(yaml: &str) -> Vec<ElementRecord> {
    let mut out = Vec::new();
    let mut current: Option<Vec<String>> = None;

    let flush = |out: &mut Vec<ElementRecord>, lines: Vec<String>| {
        if lines.is_empty() { return; }
        let raw = lines.join("\n");
        let mut rec = ElementRecord {
            raw: raw.clone(),
            content: String::new(),
            etype: String::new(),
            id: None,
            bounds: None,
            clickable: false,
        };
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }
            let key = field_key(trimmed);
            let val = extract_value(trimmed);
            match key {
                "content" => rec.content = val,
                "type" => rec.etype = val,
                "id" | "platform_id" => { if !val.is_empty() { rec.id = Some(val); } }
                "clickable" => rec.clickable = val == "true",
                "bounds" => rec.bounds = parse_bounds(&val),
                _ => {}
            }
        }
        // Keep behavior identical to prior crawl flush: any of content/type/id
        // present means the chunk is real.
        if !rec.content.is_empty() || !rec.etype.is_empty() || rec.id.is_some() {
            out.push(rec);
        }
    };

    for line in yaml.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") && trimmed.contains(':') {
            if let Some(lines) = current.take() {
                flush(&mut out, lines);
            }
            current = Some(vec![trimmed.trim_start_matches('-').trim_start().to_string()]);
        } else if let Some(ref mut lines) = current {
            lines.push(trimmed.to_string());
        }
    }
    if let Some(lines) = current.take() {
        flush(&mut out, lines);
    }
    out
}
