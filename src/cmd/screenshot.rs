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
}

pub fn run(dev_name: Option<&str>, args: ScreenshotArgs) -> Result<(), String> {
    let devices = Registry::load()?;
    let dev = if devices.is_empty() && dev_name.is_none() {
        None
    } else {
        let (_, d) = Registry::resolve(dev_name, &devices)?;
        Some(d)
    };

    let png = adb::adb_raw(dev.as_ref(), &["exec-out", "screencap", "-p"])?;
    std::fs::write(&args.output, &png)
        .map_err(|e| format!("failed to write {}: {e}", args.output))?;

    // Downscale for Claude's image limit
    let _ = std::process::Command::new("sips")
        .args(["-Z", DOWNSCALE_MAX_DIM, &args.output])
        .output();

    println!("{}", args.output);
    Ok(())
}
