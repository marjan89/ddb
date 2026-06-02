//! Shared low-level helpers for the semantic-agent /semantic YAML response.
//!
//! Both the test runner (`cmd::test_element::get_semantic_elements`) and the
//! crawl loop consume `split_elements` so element boundaries agree across the
//! toolchain. The per-chunk field extractors (`chunk_top_field`, `chunk_bounds`)
//! are shared so crawl-side typed parsing matches the test runner's substring
//! semantics — one mechanism per concern.

/// Split the agent YAML into raw chunks delimited by top-level list items
/// (`\n- ` marker). Byte-compatible with the format the test runner's
/// substring searches expect.
pub fn split_elements(yaml: &str) -> Vec<String> {
    yaml.split("\n- ").map(|s| s.to_string()).collect()
}

/// Extract a top-level `key: value` from a chunk, value-only (quote-stripped).
///
/// After [`split_elements`] consumes the leading `\n- ` delimiter, the first
/// line of a chunk sits at column 0 (the dash + space was stripped) while
/// every subsequent top-level field keeps its original 2-space indent. This
/// function treats either column 0 OR the indent of the second non-empty line
/// as top-level; deeper indents are skipped so nested children (e.g.
/// `bounds.{x,y,w,h}`) cannot shadow same-name top-level fields.
pub fn chunk_top_field(chunk: &str, key: &str) -> Option<String> {
    let mut field_indent: Option<usize> = None;
    let mut seen_first = false;
    for line in chunk.lines() {
        if line.trim().is_empty() { continue; }
        let indent = line.chars().take_while(|c| *c == ' ').count();
        if !seen_first {
            seen_first = true;
        } else if field_indent.is_none() {
            field_indent = Some(indent);
        }
        let is_top = indent == 0 || Some(indent) == field_indent;
        if !is_top { continue; }
        let trimmed = line.trim_start().trim_start_matches('-').trim_start();
        if let Some(rest) = trimmed.strip_prefix(&format!("{key}:")) {
            return Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

/// Extract a nested `bounds:` block as `[left, top, right, bottom]` (pixels).
///
/// Accepts three shapes the agent has been observed to emit:
///   - inline array: `bounds: [l, t, r, b]`
///   - nested mapping `{x, y, w, h}`
///   - nested mapping `{left, top, right, bottom}`
pub fn chunk_bounds(chunk: &str) -> Option<[i32; 4]> {
    // Inline form first.
    if let Some(line) = chunk.lines().find(|l| {
        l.trim_start().trim_start_matches('-').trim_start().starts_with("bounds:")
    }) {
        if let Some(after) = line.split_once("bounds:").map(|(_, v)| v.trim()) {
            if after.starts_with('[') && after.ends_with(']') {
                let nums: Vec<i32> = after.trim_start_matches('[').trim_end_matches(']')
                    .split(',').filter_map(|p| p.trim().parse().ok()).collect();
                if nums.len() == 4 { return Some([nums[0], nums[1], nums[2], nums[3]]); }
            }
        }
    }
    // Nested form.
    let mut in_bounds = false;
    let mut field_indent: Option<usize> = None;
    let mut seen_first = false;
    let mut bounds_indent: Option<usize> = None;
    let mut x = None; let mut y = None; let mut w = None; let mut h = None;
    let mut l = None; let mut t = None; let mut r = None; let mut b = None;
    for line in chunk.lines() {
        if line.trim().is_empty() { continue; }
        let indent = line.chars().take_while(|c| *c == ' ').count();
        if !seen_first { seen_first = true; }
        else if field_indent.is_none() { field_indent = Some(indent); }
        let trimmed = line.trim_start();
        if !in_bounds {
            let is_top = indent == 0 || Some(indent) == field_indent;
            if is_top && trimmed.trim_start_matches('-').trim_start().starts_with("bounds:") {
                in_bounds = true;
                bounds_indent = Some(indent);
            }
            continue;
        }
        let b_ind = bounds_indent.unwrap();
        if indent <= b_ind { break; }
        let key = trimmed.split_once(':').map(|(k, _)| k.trim()).unwrap_or("");
        let val = trimmed.split_once(':').and_then(|(_, v)| v.trim().parse::<i32>().ok());
        match key {
            "x" => x = val, "y" => y = val, "w" => w = val, "h" => h = val,
            "left" => l = val, "top" => t = val, "right" => r = val, "bottom" => b = val,
            _ => {}
        }
    }
    if let (Some(l), Some(t), Some(r), Some(b)) = (l, t, r, b) { return Some([l, t, r, b]); }
    if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) { return Some([x, y, x + w, y + h]); }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_elements_treats_leading_dash_as_boundary() {
        let yaml = "header:\n  foo: 1\n- id: a\n  clickable: true\n- id: b\n  clickable: false\n";
        let chunks = split_elements(yaml);
        // 1 prelude + 2 elements
        assert_eq!(chunks.len(), 3);
        assert!(chunks[1].starts_with("id: a"));
        assert!(chunks[2].starts_with("id: b"));
    }

    #[test]
    fn chunk_top_field_handles_first_line_at_column_zero() {
        let chunk = "id: foo\n  platform_id: foo_p\n  clickable: true\n  bounds:\n    x: 1\n    y: 2\n    w: 3\n    h: 4";
        assert_eq!(chunk_top_field(chunk, "id"), Some("foo".into()));
        assert_eq!(chunk_top_field(chunk, "platform_id"), Some("foo_p".into()));
        assert_eq!(chunk_top_field(chunk, "clickable"), Some("true".into()));
    }

    #[test]
    fn chunk_top_field_does_not_shadow_with_nested_type() {
        // outer type=image, image.type=loaded — must not return loaded
        let chunk = "id: foo\n  type: image\n  image:\n    type: loaded";
        assert_eq!(chunk_top_field(chunk, "type"), Some("image".into()));
    }

    #[test]
    fn chunk_bounds_decodes_xywh() {
        let chunk = "id: foo\n  bounds:\n    x: 45\n    y: 469\n    w: 300\n    h: 393";
        assert_eq!(chunk_bounds(chunk), Some([45, 469, 345, 862]));
    }

    #[test]
    fn chunk_bounds_decodes_ltrb() {
        let chunk = "id: foo\n  bounds:\n    left: 10\n    top: 20\n    right: 100\n    bottom: 200";
        assert_eq!(chunk_bounds(chunk), Some([10, 20, 100, 200]));
    }

    #[test]
    fn chunk_bounds_decodes_inline_array() {
        let chunk = "id: foo\n  bounds: [0, 0, 1080, 2340]";
        assert_eq!(chunk_bounds(chunk), Some([0, 0, 1080, 2340]));
    }
}
