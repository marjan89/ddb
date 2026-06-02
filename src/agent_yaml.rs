//! Shared low-level helpers for the semantic-agent /semantic YAML response.
//!
//! Currently exposes a single function — `split_elements` — that splits the
//! agent body into raw per-element chunks. Both the test runner
//! (`cmd::test_element::get_semantic_elements`) and the crawl loop consume
//! it so element boundaries agree across the toolchain.
//!
//! Structured parsing lives at the call site: each consumer extracts the
//! fields it needs from a chunk with direct string scans. A previous
//! serde_yaml-based `parse_elements` lived here but produced a parallel
//! implementation that drifted out of sync with the test path. Removed —
//! one mechanism per concern.

/// Split the agent YAML into raw chunks delimited by top-level list items
/// (`\n- ` marker). Byte-compatible with the format the test runner's
/// substring searches expect.
pub fn split_elements(yaml: &str) -> Vec<String> {
    yaml.split("\n- ").map(|s| s.to_string()).collect()
}
