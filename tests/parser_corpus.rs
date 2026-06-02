//! Layer-1 regression suite: real `/semantic` responses captured from device
//! get parsed with the live `agent_yaml` helpers; per-fixture invariants are
//! locked in by `assert_eq!` so any future parser drift (indent rules,
//! field names, bounds shape) fails CI before reaching the device.
//!
//! Fixtures live at `tests/fixtures/semantic/*.yaml`. To add one, capture a
//! response via `curl -s http://127.0.0.1:9876/semantic > tests/fixtures/...`,
//! count `^- id:` / `^  clickable: true` / `^  platform_id:` lines from the
//! shell, and add a `case!()` entry below.

use std::path::PathBuf;

use ddb::agent_yaml::{chunk_bounds, chunk_top_field, split_elements};

struct Counts {
    element_count: usize,
    clickable_count: usize,
    with_id: usize,
    with_platform_id: usize,
    with_bounds: usize,
}

fn count_from_fixture(name: &str) -> Counts {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/semantic")
        .join(name);
    let yaml = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing fixture {}: {e}", path.display()));

    // split_elements yields a leading prelude chunk before the first `- ` —
    // skip it so the element count matches the raw `^- id:` line grep.
    let chunks = split_elements(&yaml);
    let elements: Vec<&String> = chunks.iter().skip(1).collect();

    let element_count = elements.len();
    let mut clickable_count = 0;
    let mut with_id = 0;
    let mut with_platform_id = 0;
    let mut with_bounds = 0;
    for chunk in &elements {
        if chunk_top_field(chunk, "clickable")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            clickable_count += 1;
        }
        if chunk_top_field(chunk, "id").is_some() {
            with_id += 1;
        }
        if chunk_top_field(chunk, "platform_id").is_some() {
            with_platform_id += 1;
        }
        if chunk_bounds(chunk).is_some() {
            with_bounds += 1;
        }
    }
    Counts { element_count, clickable_count, with_id, with_platform_id, with_bounds }
}

macro_rules! corpus_case {
    ($name:ident, $file:literal, count = $count:expr, clickable = $clickable:expr,
     with_id = $id:expr, with_platform_id = $pid:expr, with_bounds = $bounds:expr) => {
        #[test]
        fn $name() {
            let c = count_from_fixture($file);
            assert_eq!(c.element_count, $count, "element_count drift in {}", $file);
            assert_eq!(c.clickable_count, $clickable, "clickable_count drift in {}", $file);
            assert_eq!(c.with_id, $id, "with_id drift in {}", $file);
            assert_eq!(c.with_platform_id, $pid, "with_platform_id drift in {}", $file);
            assert_eq!(c.with_bounds, $bounds, "with_bounds drift in {}", $file);
        }
    };
}

// Numbers below were captured against the live a54 + mi-a2 NK builds at
// commit 5e6e8c3. Update only with a matching fixture replacement.

corpus_case!(
    a54_discover,
    "a54_discover.yaml",
    count = 74, clickable = 42, with_id = 74, with_platform_id = 59, with_bounds = 74
);

corpus_case!(
    mi_a2_discover,
    "mi_a2_discover.yaml",
    count = 68, clickable = 38, with_id = 68, with_platform_id = 54, with_bounds = 68
);

corpus_case!(
    a54_coldlaunch,
    "a54_coldlaunch.yaml",
    count = 29, clickable = 0, with_id = 29, with_platform_id = 21, with_bounds = 29
);
