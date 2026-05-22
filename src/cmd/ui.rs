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
}

pub fn run(dev_name: Option<&str>, args: UiArgs) -> Result<(), String> {
    let devices = Registry::load()?;
    let dev = if devices.is_empty() && dev_name.is_none() {
        None
    } else {
        let (_, d) = Registry::resolve(dev_name, &devices)?;
        Some(d)
    };

    // Dump UI hierarchy
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
        let yaml = run_semantic(dev.as_ref(), &xml, &args)?;
        if let Some(ref path) = args.output {
            std::fs::write(path, &yaml).map_err(|e| format!("write error: {e}"))?;
            eprintln!("wrote {}", path);
        } else {
            print!("{yaml}");
        }
        return Ok(());
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

fn run_semantic(dev: Option<&Device>, xml: &str, args: &UiArgs) -> Result<String, String> {
    let agent_yaml = if args.no_agent {
        None
    } else {
        try_agent(dev)
    };

    if let Some(ref agent_yaml) = agent_yaml {
        if args.source_root.is_some() {
            // Hybrid: agent data + source resolution for font family and icons
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
                            // Font family from source (agent returns "sans-serif")
                            if let Some(ref src_font) = attrs.font {
                                if let Some(ref mut ef) = elem.font {
                                    if ef.family == "sans-serif" || ef.family.is_empty() {
                                        ef.family = src_font.family.clone();
                                    }
                                } else {
                                    elem.font = Some(src_font.clone());
                                }
                            }
                            // Icons from source (agent has none)
                            if elem.icon.is_none() {
                                elem.icon = attrs.icon;
                            }
                            // Background from source if agent doesn't have it
                            if elem.background.is_none() {
                                elem.background = attrs.background_color;
                            }
                            // Corner radius from source if agent doesn't have it
                            if elem.corner_radius.is_none() {
                                elem.corner_radius = attrs.corner_radius;
                            }
                        }
                    }
                }
            }

            return serde_yaml::to_string(&agent_schema)
                .map_err(|e| format!("yaml error: {e}"));
        }

        // Agent only, no source resolution
        eprintln!("source: semantic-agent");
        return Ok(agent_yaml.clone());
    }

    // Fallback: uiautomator + source resolution
    eprintln!("source: uiautomator + resource resolution");
    let schema = crate::semantic::extract(dev, xml, args.source_root.as_deref())?;
    serde_yaml::to_string(&schema).map_err(|e| format!("yaml error: {e}"))
}

fn try_agent(dev: Option<&Device>) -> Option<String> {
    let _ = adb::adb(dev, &["forward", "tcp:9876", "tcp:9876"]).ok()?;

    let resp = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "1", "http://localhost:9876/semantic"])
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
