use clap::Args;

use crate::registry::Registry;

#[derive(Args)]
pub struct AdbArgs {
    /// Arguments passed to adb
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

pub fn run(dev_name: Option<&str>, args: AdbArgs) -> Result<(), String> {
    let mut cmd = std::process::Command::new("adb");

    // Inject -s from registry if we have a device
    let devices = Registry::load()?;
    if !devices.is_empty() || dev_name.is_some() {
        let (_, dev) = Registry::resolve(dev_name, &devices)?;
        cmd.arg("-s").arg(dev.transport_id());
    }

    for arg in &args.args {
        cmd.arg(arg);
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to run adb: {e}"))?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
