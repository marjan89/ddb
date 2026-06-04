use clap::Args;

use crate::adb;
use crate::registry::{Device, Registry};
use crate::ui_parser;

#[derive(Args)]
pub struct UiArgs {
    /// Output raw XML instead of compact view
    #[arg(long)]
    pub raw: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Semantic extraction: produces common schema YAML
    #[arg(long)]
    pub semantic: bool,

    /// Android source tree root for resource resolution (used with --semantic)
    #[arg(long)]
    pub source_root: Option<String>,

    /// Output file path (used with --semantic)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Force uiautomator (skip agent auto-detect)
    #[arg(long)]
    pub no_agent: bool,

    /// Catalogue root path — auto-update manifest.yaml after writing
    #[arg(long)]
    pub catalogue: Option<String>,
}

pub fn run(dev_name: Option<&str>, args: UiArgs) -> Result<(), String> {
    let devices = Registry::load()?;
    let dev = if devices.is_empty() && dev_name.is_none() {
        None
    } else {
        let (_, d) = Registry::resolve(dev_name, &devices)?;
        Some(d)
    };

    // TD-33: --semantic short-circuits the uiautomator dump when the
    // in-process agent is available. The dump is ~2s on a non-trivial
    // tree and is unused on the agent-success path; only the fallback
    // (uiautomator+resource resolution) consumes the xml.
    if args.semantic && !args.no_agent {
        if let Some(agent_yaml) = try_agent(dev.as_ref()) {
            let yaml = apply_source_resolution(&agent_yaml, &args)?;
            return write_or_print_semantic(&yaml, &args);
        }
        // Agent unavailable — fall through to dump + fallback path.
    }

    // Dump UI hierarchy (needed for --raw, --json, default compact view,
    // and the --semantic fallback when agent is unavailable / --no-agent).
    adb::shell(dev.as_ref(), &["uiautomator", "dump", "/sdcard/ui.xml"])?;
    let xml = adb::shell(dev.as_ref(), &["cat", "/sdcard/ui.xml"])?;

    if !xml.contains("<hierarchy") {
        return Err("empty dump — screen may be locked or animating".to_string());
    }

    if args.raw {
        println!("{xml}");
        return Ok(());
    }

    if args.semantic {
        // Agent-unavailable fallback: uiautomator + resource resolution.
        eprintln!("source: uiautomator + resource resolution");
        let schema = crate::semantic::extract(dev.as_ref(), &xml, args.source_root.as_deref())?;
        let yaml = serde_yaml::to_string(&schema).map_err(|e| format!("yaml error: {e}"))?;
        return write_or_print_semantic(&yaml, &args);
    }

    let elements = ui_parser::parse(&xml);

    if args.json {
        let json =
            serde_json::to_string_pretty(&elements).map_err(|e| format!("json error: {e}"))?;
        println!("{json}");
        return Ok(());
    }

    for e in &elements {
        let marker = if e.clickable { '●' } else { '○' };
        let id_part = if e.id.is_empty() {
            String::new()
        } else {
            format!("  [{}]", e.id)
        };
        println!("{marker} ({:4},{:4})  {}{id_part}", e.x, e.y, e.label);
    }
    Ok(())
}

/// Apply source-tree resource resolution on top of the agent yaml when
/// --source-root is set; otherwise return the agent yaml unchanged.
fn apply_source_resolution(agent_yaml: &str, args: &UiArgs) -> Result<String, String> {
    if args.source_root.is_none() {
        eprintln!("source: semantic-agent");
        return Ok(agent_yaml.to_string());
    }

    eprintln!("source: hybrid (agent + source resolution)");
    let mut agent_schema: crate::semantic::SemanticSchema =
        serde_yaml::from_str(agent_yaml)
            .map_err(|e| format!("parse agent yaml: {e}"))?;

    let res_ctx = args
        .source_root
        .as_deref()
        .map(crate::semantic::resource::ResourceContext::load);

    if let Some(ref ctx) = res_ctx {
        for elem in &mut agent_schema.elements {
            if let Some(ref pid) = elem.platform_id {
                if let Some(attrs) = ctx.resolve_view(pid) {
                    if let Some(ref src_font) = attrs.font {
                        if let Some(ref mut ef) = elem.font {
                            if ef.family == "sans-serif" || ef.family.is_empty() {
                                ef.family = src_font.family.clone();
                            }
                        } else {
                            elem.font = Some(src_font.clone());
                        }
                    }
                    if elem.icon.is_none() {
                        elem.icon = attrs.icon;
                    }
                    if elem.background.is_none() {
                        elem.background = attrs.background_color;
                    }
                    if elem.corner_radius.is_none() {
                        elem.corner_radius = attrs.corner_radius;
                    }
                }
            }
        }
    }

    serde_yaml::to_string(&agent_schema).map_err(|e| format!("yaml error: {e}"))
}

/// Write the semantic yaml to --output (with catalogue manifest update
/// when applicable) or print to stdout.
fn write_or_print_semantic(yaml: &str, args: &UiArgs) -> Result<(), String> {
    let Some(ref path) = args.output else {
        print!("{yaml}");
        return Ok(());
    };

    let output_path = std::path::Path::new(path);

    let cat_info = args
        .catalogue
        .as_deref()
        .map(|c| {
            let key = crate::catalogue::detect_catalogue_path(path)
                .map(|(_, k)| k);
            (std::path::PathBuf::from(c), key)
        })
        .or_else(|| {
            crate::catalogue::detect_catalogue_path(path)
                .map(|(root, key)| (root, Some(key)))
        });

    let history_count = if output_path.exists() {
        crate::catalogue::archive_existing(output_path)?
    } else {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create dirs: {e}"))?;
        }
        0
    };

    std::fs::write(path, yaml).map_err(|e| format!("write error: {e}"))?;
    eprintln!("wrote {}", path);

    if let Some((cat_root, Some(entry_key))) = cat_info {
        let schema: crate::semantic::SemanticSchema =
            serde_yaml::from_str(yaml)
                .map_err(|e| format!("count elements: {e}"))?;
        let count = schema.elements.len() as u64;
        crate::catalogue::update_manifest_semantic(
            &cat_root,
            &entry_key,
            count,
            history_count,
        )?;
    }
    Ok(())
}

fn try_agent(dev: Option<&Device>) -> Option<String> {
    let port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| "9876".into());
    let _ = adb::adb(dev, &["forward", &format!("tcp:{port}"), "tcp:9876"]).ok()?;

    let resp = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "1", &format!("http://localhost:{port}/semantic")])
        .output()
        .ok()?;

    if !resp.status.success() {
        return None;
    }

    let body = String::from_utf8_lossy(&resp.stdout);
    if body.contains("screen:") && body.contains("elements:") {
        Some(body.into_owned())
    } else {
        None
    }
}
