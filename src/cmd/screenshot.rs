use clap::Args;

use crate::adb;
use crate::registry::Registry;

/// Max dimension for downscaled screenshots (Claude's image limit is ~2000px)
const DOWNSCALE_MAX_DIM: &str = "1200";

#[derive(Args)]
pub struct ScreenshotArgs {
    /// Output file path (default: /tmp/screen.png)
    #[arg(default_value = "/tmp/screen.png")]
    pub output: String,

    /// Catalogue root path — auto-update manifest.yaml after writing
    #[arg(long)]
    pub catalogue: Option<String>,
}

pub fn run(dev_name: Option<&str>, args: ScreenshotArgs) -> Result<(), String> {
    let devices = Registry::load()?;
    let dev = if devices.is_empty() && dev_name.is_none() {
        None
    } else {
        let (_, d) = Registry::resolve(dev_name, &devices)?;
        Some(d)
    };

    let output_path = std::path::Path::new(&args.output);

    // Detect catalogue path from output or explicit flag
    let cat_info = args
        .catalogue
        .as_deref()
        .map(|c| {
            let key = crate::catalogue::detect_catalogue_path(&args.output)
                .map(|(_, k)| k);
            (std::path::PathBuf::from(c), key)
        })
        .or_else(|| {
            crate::catalogue::detect_catalogue_path(&args.output)
                .map(|(root, key)| (root, Some(key)))
        });

    // Archive existing screenshot before overwriting
    if output_path.exists() {
        let _ = crate::catalogue::archive_existing(output_path);
    } else if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let png = adb::adb_raw(dev.as_ref(), &["exec-out", "screencap", "-p"])?;
    std::fs::write(&args.output, &png)
        .map_err(|e| format!("failed to write {}: {e}", args.output))?;

    // Downscale for Claude's image limit
    let _ = std::process::Command::new("sips")
        .args(["-Z", DOWNSCALE_MAX_DIM, &args.output])
        .output();

    // Update manifest if catalogue path detected
    if let Some((cat_root, Some(entry_key))) = cat_info {
        let _ = crate::catalogue::update_manifest_screenshot(&cat_root, &entry_key);
    }

    println!("{}", args.output);
    Ok(())
}
