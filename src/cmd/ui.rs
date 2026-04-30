use clap::Args;

use crate::adb;
use crate::registry::Registry;
use crate::ui_parser;

#[derive(Args)]
pub struct UiArgs {
    /// Output raw XML instead of compact view
    #[arg(long)]
    pub raw: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
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
