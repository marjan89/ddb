use clap::{Args, Subcommand};
use chrono::Local;

use crate::adb;
use crate::registry::{Device, Registry};

const DEFAULT_ADB_PORT: u16 = 5555;
const TCP_SWITCH_SETTLE_SECS: u64 = 2;
const CONNECT_SETTLE_SECS: u64 = 1;

#[derive(Args)]
pub struct DevicesArgs {
    #[command(subcommand)]
    command: Option<DevicesCommand>,
}

#[derive(Subcommand)]
enum DevicesCommand {
    /// List enrolled devices
    List,
    /// Check device connection status
    Status {
        /// Device name (all if omitted)
        name: Option<String>,
    },
    /// Enroll a new device (or re-enroll with --force)
    Add {
        /// Short name for the device
        name: String,
        /// ADB serial number
        #[arg(long)]
        serial: String,
        /// Device model description
        #[arg(long)]
        model: String,
        /// Android version
        #[arg(long)]
        android: String,
        /// SDK level
        #[arg(long)]
        sdk: u32,
        /// WiFi IP address
        #[arg(long)]
        wifi_ip: Option<String>,
        /// ADB wireless port
        #[arg(long, default_value = "5555")]
        adb_port: Option<u16>,
        /// Overwrite an existing enrollment with the same name
        #[arg(long)]
        force: bool,
    },
    /// Remove an enrolled device
    Remove {
        /// Device name
        name: String,
    },
    /// Connect to device wirelessly
    Connect {
        /// Device name
        name: String,
    },
    /// Disconnect wireless connection
    Disconnect {
        /// Device name
        name: String,
    },
}

pub fn run(args: DevicesArgs) -> Result<(), String> {
    match args.command.unwrap_or(DevicesCommand::List) {
        DevicesCommand::List => list(),
        DevicesCommand::Status { name } => status(name.as_deref()),
        DevicesCommand::Add {
            name,
            serial,
            model,
            android,
            sdk,
            wifi_ip,
            adb_port,
            force,
        } => add(&name, serial, model, android, sdk, wifi_ip, adb_port, force),
        DevicesCommand::Remove { name } => remove(&name),
        DevicesCommand::Connect { name } => connect(&name),
        DevicesCommand::Disconnect { name } => disconnect(&name),
    }
}

fn list() -> Result<(), String> {
    let devices = Registry::load()?;
    if devices.is_empty() {
        println!("No devices enrolled.");
        return Ok(());
    }

    println!(
        "{:<12} {:<35} {:<18} {}",
        "Name", "Model", "IP", "Serial"
    );
    println!("{}", "-".repeat(85));
    for (name, dev) in &devices {
        let addr = match dev.wifi_addr() {
            Some(a) => a,
            None => "-".to_string(),
        };
        println!("{name:<12} {:<35} {addr:<18} {}", dev.model, dev.serial);
    }
    Ok(())
}

fn status(name: Option<&str>) -> Result<(), String> {
    let devices = Registry::load()?;
    let connected = adb::connected_serials()?;

    let names: Vec<String> = match name {
        Some(n) => vec![n.to_string()],
        None => devices.keys().cloned().collect(),
    };

    for n in &names {
        let (_, dev) = Registry::resolve(Some(n), &devices)?;
        println!("== {n} ({}) ==", dev.transport_id());

        // WiFi
        if let Some(addr) = dev.wifi_addr() {
            let wifi_ok = connected.iter().any(|(s, st)| s == &addr && st == "device");
            println!(
                "  WiFi:  {}",
                if wifi_ok { "CONNECTED" } else { "not connected" }
            );
        }

        // USB
        let usb_ok = connected
            .iter()
            .any(|(s, st)| s == &dev.serial && st == "device");
        println!(
            "  USB:   {}",
            if usb_ok { "CONNECTED" } else { "not connected" }
        );

        // Ping
        if let Some(ip) = &dev.wifi_ip {
            let ping_ok = std::process::Command::new("ping")
                .args(["-c", "1", "-W", "1", ip])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            println!(
                "  Ping:  {}",
                if ping_ok { "reachable" } else { "unreachable" }
            );
        }

        // Daemon
        let daemon_loaded = std::process::Command::new("launchctl")
            .args(["list"])
            .output()
            .map(|o| {
                let out = String::from_utf8_lossy(&o.stdout);
                out.contains(&format!("ddb-heartbeat-{n}"))
            })
            .unwrap_or(false);
        println!(
            "  Daemon: {}",
            if daemon_loaded {
                "LOADED"
            } else {
                "not loaded"
            }
        );

        println!();
    }
    Ok(())
}

fn add(
    name: &str,
    serial: String,
    model: String,
    android: String,
    sdk: u32,
    wifi_ip: Option<String>,
    adb_port: Option<u16>,
    force: bool,
) -> Result<(), String> {
    let mut devices = Registry::load()?;
    let existing = devices.get(name).cloned();
    if existing.is_some() && !force {
        return Err(format!(
            "device '{name}' already enrolled (use --force to re-enroll)"
        ));
    }

    let enrolled = existing
        .as_ref()
        .map(|d| d.enrolled.clone())
        .unwrap_or_else(|| Local::now().format("%Y-%m-%d").to_string());
    let dev = Device {
        serial,
        model,
        android,
        sdk,
        wifi_ip,
        adb_port,
        enrolled,
    };

    devices.insert(name.to_string(), dev);
    Registry::save(&devices)?;
    println!(
        "{} '{name}'.",
        if existing.is_some() {
            "re-enrolled"
        } else {
            "enrolled"
        }
    );
    Ok(())
}

fn remove(name: &str) -> Result<(), String> {
    let mut devices = Registry::load()?;
    if devices.remove(name).is_none() {
        return Err(format!("device '{name}' not found"));
    }
    Registry::save(&devices)?;
    println!("removed '{name}'.");
    Ok(())
}

fn connect(name: &str) -> Result<(), String> {
    let devices = Registry::load()?;
    let (_, dev) = Registry::resolve(Some(name), &devices)?;

    let addr = dev
        .wifi_addr()
        .ok_or_else(|| format!("{name}: no wifi_ip configured"))?;

    let connected = adb::connected_serials()?;
    if connected.iter().any(|(s, st)| s == &addr && st == "device") {
        println!("{name}: already connected wirelessly at {addr}");
        return Ok(());
    }

    // If USB connected, switch to TCP mode first
    if connected
        .iter()
        .any(|(s, st)| s == &dev.serial && st == "device")
    {
        println!("{name}: switching USB to TCP mode...");
        let port = dev.adb_port.unwrap_or(DEFAULT_ADB_PORT);
        adb::adb(Some(&dev), &["tcpip", &port.to_string()])?;
        std::thread::sleep(std::time::Duration::from_secs(TCP_SWITCH_SETTLE_SECS));
    }

    println!("{name}: connecting to {addr}...");
    adb::adb(None, &["connect", &addr])?;
    std::thread::sleep(std::time::Duration::from_secs(CONNECT_SETTLE_SECS));

    let connected = adb::connected_serials()?;
    if connected.iter().any(|(s, st)| s == &addr && st == "device") {
        println!("{name}: wireless ADB connected.");
        Ok(())
    } else {
        Err(format!(
            "{name}: connection failed. Is the device on WiFi at {}?",
            dev.wifi_ip.as_deref().unwrap_or("?")
        ))
    }
}

fn disconnect(name: &str) -> Result<(), String> {
    let devices = Registry::load()?;
    let (_, dev) = Registry::resolve(Some(name), &devices)?;
    let addr = dev
        .wifi_addr()
        .ok_or_else(|| format!("{name}: no wifi_ip configured"))?;
    adb::adb(None, &["disconnect", &addr])?;
    println!("{name}: disconnected.");
    Ok(())
}
