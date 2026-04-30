use clap::Args;

use crate::config::Config;
use crate::registry::Registry;

#[derive(Args)]
pub struct MirrorArgs {
    /// Extra args passed to scrcpy
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub extra: Vec<String>,
}

pub fn run(dev_name: Option<&str>, args: MirrorArgs) -> Result<(), String> {
    let config = Config::load()?;
    let devices = Registry::load()?;

    let serial = if devices.is_empty() && dev_name.is_none() {
        None
    } else {
        let (_, dev) = Registry::resolve(dev_name, &devices)?;
        Some(dev.transport_id())
    };

    let mut cmd = std::process::Command::new(&config.scrcpy_path);
    cmd.arg("--legacy-paste");
    if let Some(ref s) = serial {
        cmd.arg("-s").arg(s);
    }
    for arg in &args.extra {
        cmd.arg(arg);
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to run scrcpy: {e}"))?;

    if !status.success() {
        return Err("scrcpy exited with error".to_string());
    }
    Ok(())
}
