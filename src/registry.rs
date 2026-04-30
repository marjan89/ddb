use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub serial: String,
    pub model: String,
    pub android: String,
    pub sdk: u32,
    pub wifi_ip: Option<String>,
    pub adb_port: Option<u16>,
    pub enrolled: String,
}

impl Device {
    /// The identifier to pass to `adb -s`. Prefers wifi address if available.
    pub fn transport_id(&self) -> String {
        match (&self.wifi_ip, self.adb_port) {
            (Some(ip), Some(port)) => format!("{ip}:{port}"),
            _ => self.serial.clone(),
        }
    }

    pub fn wifi_addr(&self) -> Option<String> {
        match (&self.wifi_ip, self.adb_port) {
            (Some(ip), Some(port)) => Some(format!("{ip}:{port}")),
            _ => None,
        }
    }
}

pub type DeviceMap = BTreeMap<String, Device>;

pub struct Registry;

impl Registry {
    pub fn path() -> PathBuf {
        Config::dir().join("devices.toml")
    }

    pub fn load() -> Result<DeviceMap, String> {
        let path = Self::path();
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("invalid devices.toml: {e}"))
    }

    pub fn save(devices: &DeviceMap) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }
        let text =
            toml::to_string_pretty(devices).map_err(|e| format!("serialize devices: {e}"))?;
        fs::write(&path, text).map_err(|e| format!("failed to write {}: {e}", path.display()))
    }

    /// Resolve a device by name. If name is None, auto-select if exactly one device exists.
    pub fn resolve(name: Option<&str>, devices: &DeviceMap) -> Result<(String, Device), String> {
        match name {
            Some(n) => match devices.get(n) {
                Some(dev) => Ok((n.to_string(), dev.clone())),
                None => {
                    let available: Vec<&str> = devices.keys().map(|s| s.as_str()).collect();
                    Err(format!(
                        "unknown device '{n}'. available: {}",
                        if available.is_empty() {
                            "(none)".to_string()
                        } else {
                            available.join(", ")
                        }
                    ))
                }
            },
            None => {
                if devices.len() == 1 {
                    let (name, dev) = devices.iter().next().unwrap();
                    Ok((name.clone(), dev.clone()))
                } else if devices.is_empty() {
                    Err("no devices enrolled. run: ddb devices add".to_string())
                } else {
                    let available: Vec<&str> = devices.keys().map(|s| s.as_str()).collect();
                    Err(format!(
                        "multiple devices enrolled, specify one with -d: {}",
                        available.join(", ")
                    ))
                }
            }
        }
    }
}
