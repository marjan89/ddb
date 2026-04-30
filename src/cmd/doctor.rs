use crate::adb;
use crate::config::Config;
use crate::registry::Registry;

pub fn run() -> Result<(), String> {
    let mut ok = true;

    // Check adb
    print!("adb: ");
    match std::process::Command::new("adb").arg("version").output() {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout);
            let first = ver.lines().next().unwrap_or("ok");
            println!("{first}");
        }
        _ => {
            println!("NOT FOUND");
            ok = false;
        }
    }

    // Check scrcpy
    print!("scrcpy: ");
    match std::process::Command::new("scrcpy").arg("--version").output() {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout);
            println!("{}", ver.trim());
        }
        _ => {
            println!("not found (optional — needed for mirror)");
        }
    }

    // Config
    print!("config: ");
    match Config::load() {
        Ok(_) => println!("{}", Config::path().display()),
        Err(e) => {
            println!("error: {e}");
            ok = false;
        }
    }

    // Registry
    print!("devices: ");
    match Registry::load() {
        Ok(devices) => {
            println!(
                "{} enrolled ({})",
                devices.len(),
                Registry::path().display()
            );

            // Check each device
            let connected = adb::connected_serials().unwrap_or_default();
            for (name, dev) in &devices {
                let transport = dev.transport_id();
                let is_connected = connected
                    .iter()
                    .any(|(s, st)| (s == &transport || s == &dev.serial) && st == "device");
                println!(
                    "  {name}: {}",
                    if is_connected {
                        "connected"
                    } else {
                        "NOT connected"
                    }
                );
            }
        }
        Err(e) => {
            println!("error: {e}");
            ok = false;
        }
    }

    if ok {
        println!("\nall good.");
    } else {
        println!("\nsome issues found.");
    }
    Ok(())
}
