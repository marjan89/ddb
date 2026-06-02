//! Shared parser for the semantic-agent /semantic YAML response.
//!
//! Single source of truth for both `cmd::crawl` (structured fields) and
//! `cmd::test_element` (raw chunks for substring matching).
//!
//! Uses serde_yaml for the structured parse so nested blocks, multi-line
//! values, and arbitrary sub-objects do not confuse element boundary
//! detection. The raw-chunk splitter is preserved for the test runner's
//! substring searches and stays byte-compatible with the legacy format.

use serde_yaml::Value;

#[derive(Debug, Clone)]
pub struct ElementRecord {
    /// Raw YAML text for the element (best-effort serialization of the parsed
    /// node — used by callers that do substring matching).
    pub raw: String,
    pub content: String,
    pub etype: String,
    /// Matches either `id` or `platform_id`. `platform_id` wins when both present.
    pub id: Option<String>,
    /// Decoded as [left, top, right, bottom] in pixels.
    pub bounds: Option<[i32; 4]>,
    pub clickable: bool,
}

/// Split the agent YAML into raw chunks delimited by top-level list items.
/// Kept verbatim — the test runner's substring searches depend on the legacy
/// chunk format.
pub fn split_elements(yaml: &str) -> Vec<String> {
    yaml.split("\n- ").map(|s| s.to_string()).collect()
}

fn as_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn as_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::String(s) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn as_i32(v: &Value) -> Option<i32> {
    match v {
        Value::Number(n) => n.as_i64().map(|i| i as i32),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Extract bounds in [left, top, right, bottom] form from any of the shapes
/// the agent emits:
///   - inline array: [left, top, right, bottom]
///   - mapping {x, y, w, h}
///   - mapping {left, top, right, bottom}
fn extract_bounds(v: &Value) -> Option<[i32; 4]> {
    if let Value::Sequence(seq) = v {
        let nums: Vec<i32> = seq.iter().filter_map(as_i32).collect();
        if nums.len() == 4 { return Some([nums[0], nums[1], nums[2], nums[3]]); }
    }
    if let Value::Mapping(m) = v {
        let get = |k: &str| m.get(Value::String(k.into())).and_then(as_i32);
        if let (Some(l), Some(t), Some(r), Some(b)) = (get("left"), get("top"), get("right"), get("bottom")) {
            return Some([l, t, r, b]);
        }
        if let (Some(x), Some(y), Some(w), Some(h)) = (get("x"), get("y"), get("w"), get("h")) {
            return Some([x, y, x + w, y + h]);
        }
    }
    None
}

fn record_from_value(v: &Value) -> Option<ElementRecord> {
    let map = match v {
        Value::Mapping(m) => m,
        _ => return None,
    };

    // Iterate the mapping and dispatch by stringified key — defensive against
    // serde_yaml's Index impl quirks (different key Value variants, tagged
    // strings, etc.). This is the only field reader; do not use Mapping::get
    // here.
    let mut id_val: Option<String> = None;
    let mut platform_id_val: Option<String> = None;
    let mut content = String::new();
    let mut etype = String::new();
    let mut clickable = false;
    let mut bounds: Option<[i32; 4]> = None;

    for (k, val) in map {
        let key = match k {
            Value::String(s) => s.as_str(),
            _ => continue,
        };
        match key {
            "id" => { if let Some(s) = as_string(val) { if !s.is_empty() { id_val = Some(s); } } }
            "platform_id" => { if let Some(s) = as_string(val) { if !s.is_empty() { platform_id_val = Some(s); } } }
            "content" => { if let Some(s) = as_string(val) { content = s; } }
            "type" => { if let Some(s) = as_string(val) { etype = s; } }
            "clickable" => {
                if std::env::var("DDB_CRAWL_DEBUG").ok().as_deref() == Some("1") {
                    let dump = match val {
                        Value::Bool(b) => format!("Bool({b})"),
                        Value::String(s) => format!("String({s:?})"),
                        Value::Number(n) => format!("Number({n})"),
                        Value::Null => "Null".into(),
                        Value::Sequence(_) => "Sequence(..)".into(),
                        Value::Mapping(_) => "Mapping(..)".into(),
                        other => format!("Other({other:?})"),
                    };
                    eprintln!("    [CLICK-RAW] {}", dump);
                }
                clickable = as_bool(val);
            }
            "bounds" => { bounds = extract_bounds(val); }
            _ => {}
        }
    }

    // platform_id wins over id when both present.
    let id = platform_id_val.or(id_val);

    let raw = serde_yaml::to_string(v).unwrap_or_default();

    // Drop completely empty records.
    if id.is_none() && content.is_empty() && etype.is_empty() && bounds.is_none() && !clickable {
        return None;
    }
    Some(ElementRecord { raw, content, etype, id, bounds, clickable })
}

/// Recursively walk a parsed YAML value, collecting every mapping that looks
/// like an element (has `id`, `platform_id`, `content`, `type`, `bounds`, or
/// `clickable`). Robust to whether the agent wraps the list under a top-level
/// key or returns it bare.
fn walk(v: &Value, out: &mut Vec<ElementRecord>) {
    match v {
        Value::Sequence(seq) => {
            for item in seq {
                if let Some(rec) = record_from_value(item) {
                    out.push(rec);
                }
                // Always recurse — items may themselves contain child lists
                // (the agent flattens these but we don't rely on that).
                walk(item, out);
            }
        }
        Value::Mapping(m) => {
            for (_, val) in m {
                walk(val, out);
            }
        }
        _ => {}
    }
}

/// Parse the agent YAML body into structured element records. Handles
/// multi-document YAML by iterating every document with the Deserializer.
pub fn parse_elements(yaml: &str) -> Vec<ElementRecord> {
    use serde::Deserialize;
    let debug = std::env::var("DDB_CRAWL_DEBUG").ok().as_deref() == Some("1");
    let mut out = Vec::new();
    let mut doc_count = 0usize;
    for doc in serde_yaml::Deserializer::from_str(yaml) {
        match Value::deserialize(doc) {
            Ok(root) => {
                doc_count += 1;
                let before = out.len();
                walk(&root, &mut out);
                if debug {
                    eprintln!("  [PARSE doc {}] +{} records (total {})", doc_count, out.len() - before, out.len());
                }
            }
            Err(e) if debug => {
                eprintln!("  [PARSE doc {}] ERROR: {}", doc_count + 1, e);
            }
            Err(_) => {}
        }
    }
    if debug {
        eprintln!("  [PARSE total] {} docs, {} records, {} bytes input", doc_count, out.len(), yaml.len());
        for (i, r) in out.iter().take(3).enumerate() {
            eprintln!("    [REC {}] id={:?} content={:?} etype={:?} clickable={} bounds={:?}",
                i, r.id, r.content, r.etype, r.clickable, r.bounds);
        }
        let click_count = out.iter().filter(|r| r.clickable).count();
        let with_bounds = out.iter().filter(|r| r.bounds.is_some()).count();
        let with_id = out.iter().filter(|r| r.id.is_some()).count();
        eprintln!("    [STATS] clickable={} with_bounds={} with_id={}",
            click_count, with_bounds, with_id);
    }
    out
}
