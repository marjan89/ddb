use std::collections::HashSet;

use clap::Args;

use crate::adb;
use crate::config::Config;
use crate::registry::{DeviceMap, Registry};

#[derive(Args)]
pub struct MirrorArgs {
    /// Extra args passed to scrcpy
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub extra: Vec<String>,
}

pub fn run(dev_name: Option<&str>, args: MirrorArgs) -> Result<(), String> {
    let config = Config::load()?;
    let devices = Registry::load()?;
    let connected = adb::connected_serials().unwrap_or_default();

    let target = pick_target(dev_name, &devices, &connected)?;

    let mut cmd = std::process::Command::new(&config.scrcpy_path);
    cmd.arg("--legacy-paste");
    if let Some(ref s) = target {
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

/// Resolve the scrcpy `-s` value. None means "let adb pick" (used when nothing is
/// enrolled and nothing is attached, so scrcpy can produce its own message).
fn pick_target(
    dev_name: Option<&str>,
    devices: &DeviceMap,
    connected: &[(String, String)],
) -> Result<Option<String>, String> {
    let live_serials: Vec<&str> = connected
        .iter()
        .filter(|(_, st)| st == "device")
        .map(|(s, _)| s.as_str())
        .collect();

    if let Some(name) = dev_name {
        if let Some(dev) = devices.get(name) {
            return Ok(Some(dev.transport_id()));
        }
        if live_serials.iter().any(|s| *s == name) {
            return Ok(Some(name.to_string()));
        }
        let avail = format_candidates(&available_targets(devices, connected));
        return Err(format!(
            "unknown device '{name}'. available: {}",
            if avail.is_empty() { "(none)" } else { &avail }
        ));
    }

    let candidates = available_targets(devices, connected);
    match candidates.len() {
        0 if devices.is_empty() && live_serials.is_empty() => Ok(None),
        0 => Err("no connected devices. attach via USB or run `ddb devices connect <name>`".into()),
        1 => Ok(Some(candidates[0].1.clone())),
        _ => Err(format!(
            "multiple devices available, specify one with -d: {}",
            format_candidates(&candidates)
        )),
    }
}

/// Pairs of (enrolled-name-if-any, transport-id) for every device that is
/// currently visible to adb, plus enrolled devices that adb sees.
fn available_targets(
    devices: &DeviceMap,
    connected: &[(String, String)],
) -> Vec<(Option<String>, String)> {
    let mut result: Vec<(Option<String>, String)> = Vec::new();
    let mut covered: HashSet<String> = HashSet::new();

    for (name, dev) in devices {
        let transport = dev.transport_id();
        let is_up = connected
            .iter()
            .any(|(s, st)| (s == &transport || s == &dev.serial) && st == "device");
        if is_up {
            result.push((Some(name.clone()), transport.clone()));
            covered.insert(transport);
            covered.insert(dev.serial.clone());
        }
    }

    for (serial, state) in connected {
        if state == "device" && !covered.contains(serial) {
            result.push((None, serial.clone()));
        }
    }

    result
}

fn format_candidates(candidates: &[(Option<String>, String)]) -> String {
    candidates
        .iter()
        .map(|(name, transport)| match name {
            Some(n) => format!("{n} ({transport})"),
            None => transport.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}
