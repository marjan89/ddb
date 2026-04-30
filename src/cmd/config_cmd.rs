use clap::{Args, Subcommand};

use crate::config::Config;
use crate::registry::Registry;

#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Create config with defaults
    Init,
    /// Show current config
    Show,
    /// Set a config value
    Set {
        /// Key (adb_path, scrcpy_path, default_device)
        key: String,
        /// Value
        value: String,
    },
    /// Print config file path
    Path,
}

pub fn run(args: ConfigArgs) -> Result<(), String> {
    match args.command {
        ConfigCommand::Init => {
            let path = Config::path();
            if path.exists() {
                return Err(format!("config already exists at {}", path.display()));
            }
            Config::default().save()?;
            println!("created {}", path.display());

            // Also create empty devices.toml if missing
            let reg_path = Registry::path();
            if !reg_path.exists() {
                Registry::save(&Default::default())?;
                println!("created {}", reg_path.display());
            }
            Ok(())
        }
        ConfigCommand::Show => {
            let config = Config::load()?;
            let text =
                toml::to_string_pretty(&config).map_err(|e| format!("serialize: {e}"))?;
            println!("{text}");
            Ok(())
        }
        ConfigCommand::Set { key, value } => {
            let mut config = Config::load()?;
            match key.as_str() {
                "adb_path" => config.adb_path = value,
                "scrcpy_path" => config.scrcpy_path = value,
                "default_device" => {
                    config.default_device = if value == "none" || value.is_empty() {
                        None
                    } else {
                        Some(value)
                    };
                }
                other => return Err(format!("unknown key '{other}'")),
            }
            config.save()?;
            println!("updated.");
            Ok(())
        }
        ConfigCommand::Path => {
            println!("{}", Config::path().display());
            Ok(())
        }
    }
}
