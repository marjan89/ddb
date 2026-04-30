use clap::{Args, Subcommand};
use std::fs;
use std::path::PathBuf;

use crate::adb;
use crate::registry::Registry;

const LABEL_PREFIX: &str = "com.user.ddb-heartbeat";
const HEARTBEAT_INTERVAL_SECS: u64 = 10;
const PING_RETRY_SECS: u64 = 10;
const RECONNECT_SETTLE_SECS: u64 = 2;
const MISS_THRESHOLD: u32 = 3;

#[derive(Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    command: DaemonCommand,
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Start heartbeat daemon for a device
    Start {
        /// Device name
        name: String,
    },
    /// Stop heartbeat daemon for a device
    Stop {
        /// Device name
        name: String,
    },
    /// Check daemon status
    Status {
        /// Device name (all if omitted)
        name: Option<String>,
    },
    /// Tail heartbeat log
    Log {
        /// Device name
        name: String,
    },
    /// Run heartbeat in foreground (for launchd)
    Heartbeat {
        /// Device name
        name: String,
    },
}

pub fn run(args: DaemonArgs) -> Result<(), String> {
    match args.command {
        DaemonCommand::Start { name } => start(&name),
        DaemonCommand::Stop { name } => stop(&name),
        DaemonCommand::Status { name } => status(name.as_deref()),
        DaemonCommand::Log { name } => log_tail(&name),
        DaemonCommand::Heartbeat { name } => heartbeat(&name),
    }
}

fn label(name: &str) -> String {
    format!("{LABEL_PREFIX}-{name}")
}

fn plist_path(name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", label(name)))
}

fn log_path(name: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/ddb-{name}-heartbeat.log"))
}

fn ddb_bin() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "ddb".to_string())
}

fn start(name: &str) -> Result<(), String> {
    let devices = Registry::load()?;
    let _ = Registry::resolve(Some(name), &devices)?;

    let plist = plist_path(name);
    let label = label(name);
    let log = log_path(name);
    let bin = ddb_bin();

    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>daemon</string>
        <string>heartbeat</string>
        <string>{name}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
</dict>
</plist>"#,
        log.display(),
        log.display(),
    );

    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create LaunchAgents dir: {e}"))?;
    }
    fs::write(&plist, content)
        .map_err(|e| format!("failed to write plist: {e}"))?;

    let status = std::process::Command::new("launchctl")
        .args(["load", &plist.display().to_string()])
        .status()
        .map_err(|e| format!("launchctl: {e}"))?;

    if status.success() {
        println!("daemon started for '{name}'.");
    } else {
        return Err("launchctl load failed".to_string());
    }
    Ok(())
}

fn stop(name: &str) -> Result<(), String> {
    let plist = plist_path(name);

    if plist.exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", &plist.display().to_string()])
            .status();
        let _ = fs::remove_file(&plist);
    }

    println!("daemon stopped for '{name}'.");
    Ok(())
}

fn status(name: Option<&str>) -> Result<(), String> {
    let devices = Registry::load()?;
    let names: Vec<String> = match name {
        Some(n) => vec![n.to_string()],
        None => devices.keys().cloned().collect(),
    };

    for n in &names {
        let label = label(n);
        let loaded = std::process::Command::new("launchctl")
            .args(["list"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&label))
            .unwrap_or(false);

        let plist_exists = plist_path(n).exists();

        let state = match (loaded, plist_exists) {
            (true, _) => "RUNNING",
            (false, true) => "STOPPED (plist exists)",
            (false, false) => "not installed",
        };
        println!("{n}: {state}");
    }
    Ok(())
}

fn log_tail(name: &str) -> Result<(), String> {
    let log = log_path(name);
    if !log.exists() {
        return Err(format!("no log file at {}", log.display()));
    }

    let status = std::process::Command::new("tail")
        .args(["-f", &log.display().to_string()])
        .status()
        .map_err(|e| format!("tail: {e}"))?;

    if !status.success() {
        return Err("tail exited".to_string());
    }
    Ok(())
}

fn heartbeat(name: &str) -> Result<(), String> {
    let devices = Registry::load()?;
    let (_, dev) = Registry::resolve(Some(name), &devices)?;

    let addr = dev
        .wifi_addr()
        .ok_or_else(|| format!("{name}: no wifi_ip configured"))?;
    let ip = dev.wifi_ip.as_deref().unwrap();

    let interval = std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS);
    let mut miss_count = 0u32;

    eprintln!("[{name}] heartbeat started — {addr}");

    // Initial connect attempt
    let _ = adb::adb(None, &["connect", &addr]);

    loop {
        std::thread::sleep(interval);

        // Check reachable
        let ping_ok = std::process::Command::new("ping")
            .args(["-c", "1", "-W", "1", ip])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !ping_ok {
            eprintln!("[{name}] unreachable, waiting...");
            miss_count = 0;
            loop {
                std::thread::sleep(std::time::Duration::from_secs(PING_RETRY_SECS));
                let ok = std::process::Command::new("ping")
                    .args(["-c", "1", "-W", "1", ip])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if ok {
                    break;
                }
            }
            eprintln!("[{name}] back on network.");
            std::thread::sleep(std::time::Duration::from_secs(RECONNECT_SETTLE_SECS));
        }

        // Check ADB
        let connected = adb::connected_serials().unwrap_or_default();
        if connected.iter().any(|(s, st)| s == &addr && st == "device") {
            miss_count = 0;
            continue;
        }

        miss_count += 1;
        eprintln!("[{name}] ADB disconnected ({miss_count}/{MISS_THRESHOLD})");

        if miss_count >= MISS_THRESHOLD {
            eprintln!("[{name}] reconnecting...");
            match adb::adb(None, &["connect", &addr]) {
                Ok(out) if out.contains("connected") => {
                    eprintln!("[{name}] reconnected.");
                }
                _ => {
                    eprintln!("[{name}] reconnect failed, will retry.");
                }
            }
            miss_count = 0;
        }
    }
}
