use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub updated: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub tolerances: Option<serde_yaml::Value>,
    #[serde(default)]
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub last_captured: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elements: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regions: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_frame: Option<String>,
}

/// Detect catalogue path components from an output file path.
/// Returns (catalogue_root, entry_key) where entry_key is like "android/discover".
/// The output path must contain a `catalogue/` ancestor with `{platform}/{screen}/` inside.
pub fn detect_catalogue_path(output: &str) -> Option<(PathBuf, String)> {
    let path = Path::new(output);
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Find "catalogue" component
    let cat_idx = components.iter().position(|c| *c == "catalogue")?;

    // Need at least platform/screen after catalogue
    if cat_idx + 2 >= components.len() {
        return None;
    }

    let platform = components[cat_idx + 1];
    let screen = components[cat_idx + 2];

    // Validate platform
    if !matches!(platform, "android" | "ios" | "figma") {
        return None;
    }

    let catalogue_root: PathBuf = components[..=cat_idx].iter().collect();
    let entry_key = format!("{platform}/{screen}");

    Some((catalogue_root, entry_key))
}

pub fn load_manifest(catalogue_root: &Path) -> Result<Manifest, String> {
    let manifest_path = catalogue_root.join("manifest.yaml");
    if manifest_path.exists() {
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("read manifest: {e}"))?;
        serde_yaml::from_str(&content).map_err(|e| format!("parse manifest: {e}"))
    } else {
        Ok(Manifest {
            version: 1,
            updated: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            project: None,
            tolerances: None,
            entries: BTreeMap::new(),
        })
    }
}

pub fn save_manifest(catalogue_root: &Path, manifest: &Manifest) -> Result<(), String> {
    let manifest_path = catalogue_root.join("manifest.yaml");
    let yaml = serde_yaml::to_string(manifest).map_err(|e| format!("serialize manifest: {e}"))?;
    std::fs::write(&manifest_path, &yaml).map_err(|e| format!("write manifest: {e}"))?;
    eprintln!("updated {}", manifest_path.display());
    Ok(())
}

/// Archive existing artifact to .history/ before overwriting.
/// Returns the history count for this screen dir.
pub fn archive_existing(output_path: &Path) -> Result<u64, String> {
    let parent = output_path.parent().unwrap();
    let history_dir = parent.join(".history");

    if output_path.exists() {
        // Read timestamp from existing semantic YAML if it's a yaml file
        let ts_suffix = if output_path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
            extract_timestamp(output_path).unwrap_or_default()
        } else {
            // For non-YAML (screenshots), use file modification time
            std::fs::metadata(output_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let dt: chrono::DateTime<Utc> = t.into();
                    dt.format("%Y-%m-%dT%H%M").to_string()
                })
                .unwrap_or_default()
        };

        if !ts_suffix.is_empty() {
            std::fs::create_dir_all(&history_dir)
                .map_err(|e| format!("create .history: {e}"))?;

            let stem = output_path.file_stem().unwrap().to_str().unwrap();
            let ext = output_path.extension().unwrap().to_str().unwrap();
            let archive_name = format!("{stem}-{ts_suffix}.{ext}");
            let archive_path = history_dir.join(&archive_name);

            std::fs::rename(output_path, &archive_path)
                .map_err(|e| format!("archive {}: {e}", output_path.display()))?;
            eprintln!("archived → .history/{archive_name}");
        }
    }

    // Count history entries
    let count = if history_dir.exists() {
        std::fs::read_dir(&history_dir)
            .map(|rd| rd.count() as u64)
            .unwrap_or(0)
    } else {
        0
    };

    Ok(count)
}

fn extract_timestamp(yaml_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(yaml_path).ok()?;
    for line in content.lines().take(10) {
        let line = line.trim();
        if let Some(ts) = line.strip_prefix("timestamp:") {
            let ts = ts.trim().trim_matches('"').trim_matches('\'');
            // Convert ISO to compact form for filename
            return Some(ts.replace(':', "").replace('-', "")[..13].to_string());
        }
    }
    None
}

/// Update manifest after writing a semantic YAML.
pub fn update_manifest_semantic(
    catalogue_root: &Path,
    entry_key: &str,
    element_count: u64,
    history_count: u64,
) -> Result<(), String> {
    let mut manifest = load_manifest(catalogue_root)?;
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let entry = manifest.entries.entry(entry_key.to_string()).or_insert(Entry {
        last_captured: String::new(),
        build: None,
        elements: None,
        state: Some("default".to_string()),
        context: None,
        history_count: None,
        regions: None,
        source_frame: None,
    });

    entry.last_captured = now.clone();
    entry.elements = Some(element_count);
    entry.history_count = Some(history_count);

    manifest.updated = now;
    save_manifest(catalogue_root, &manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_catalogue_path_valid() {
        let (root, key) = detect_catalogue_path(
            "/Users/Shared/projects/Outdoors/catalogue/android/discover/semantic.yaml",
        )
        .unwrap();
        assert_eq!(root, PathBuf::from("/Users/Shared/projects/Outdoors/catalogue"));
        assert_eq!(key, "android/discover");
    }

    #[test]
    fn detect_catalogue_path_ios() {
        let (_, key) = detect_catalogue_path(
            "/some/path/catalogue/ios/site-detail/screenshot.png",
        )
        .unwrap();
        assert_eq!(key, "ios/site-detail");
    }

    #[test]
    fn detect_catalogue_path_invalid_platform() {
        assert!(detect_catalogue_path(
            "/path/catalogue/windows/screen/file.yaml"
        )
        .is_none());
    }

    #[test]
    fn detect_catalogue_path_too_short() {
        assert!(detect_catalogue_path("/path/catalogue/android").is_none());
    }

    #[test]
    fn manifest_roundtrip() {
        let dir = std::env::temp_dir().join("ddb-test-catalogue");
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join("manifest.yaml"));

        update_manifest_semantic(&dir, "android/discover", 42, 0).unwrap();

        let manifest = load_manifest(&dir).unwrap();
        assert_eq!(manifest.entries.len(), 1);
        let entry = manifest.entries.get("android/discover").unwrap();
        assert_eq!(entry.elements, Some(42));
        assert_eq!(entry.history_count, Some(0));
        assert_eq!(entry.state.as_deref(), Some("default"));

        update_manifest_screenshot(&dir, "android/discover").unwrap();
        let manifest2 = load_manifest(&dir).unwrap();
        let entry2 = manifest2.entries.get("android/discover").unwrap();
        assert_eq!(entry2.elements, Some(42));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn archive_and_count() {
        let dir = std::env::temp_dir().join("ddb-test-archive");
        std::fs::create_dir_all(&dir).unwrap();
        let yaml_path = dir.join("semantic.yaml");
        std::fs::write(&yaml_path, "timestamp: \"2026-05-24T00:49:16Z\"\nelements:\n  - id: test\n").unwrap();

        let count = archive_existing(&yaml_path).unwrap();
        assert_eq!(count, 1);
        assert!(!yaml_path.exists());
        assert!(dir.join(".history").exists());

        let history_files: Vec<_> = std::fs::read_dir(dir.join(".history"))
            .unwrap()
            .collect();
        assert_eq!(history_files.len(), 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_existing_manifest() {
        let yaml = r#"version: 1
updated: '2026-05-24T00:30:00Z'
project: naturkartan
tolerances:
  spatial: 10%
  color: 3.0
entries:
  android/discover:
    last_captured: '2026-05-24T00:54:00Z'
    elements: 60
    state: default
    regions: 7
"#;
        let manifest: Manifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.project, Some("naturkartan".to_string()));
        assert!(manifest.tolerances.is_some());
        assert_eq!(manifest.entries.len(), 1);
        let entry = manifest.entries.get("android/discover").unwrap();
        assert_eq!(entry.elements, Some(60));
        assert_eq!(entry.regions, Some(7));
    }
}

/// Update manifest timestamp after writing a screenshot (no element count change).
pub fn update_manifest_screenshot(
    catalogue_root: &Path,
    entry_key: &str,
) -> Result<(), String> {
    let mut manifest = load_manifest(catalogue_root)?;
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    if let Some(entry) = manifest.entries.get_mut(entry_key) {
        entry.last_captured = now.clone();
    } else {
        manifest.entries.insert(entry_key.to_string(), Entry {
            last_captured: now.clone(),
            build: None,
            elements: None,
            state: Some("default".to_string()),
            context: None,
            history_count: None,
            regions: None,
            source_frame: None,
        });
    }

    manifest.updated = now;
    save_manifest(catalogue_root, &manifest)
}
