use clap::{Args, Subcommand};

use crate::adb;
use crate::registry::Registry;

#[derive(Args)]
pub struct AppArgs {
    #[command(subcommand)]
    command: AppCommand,
}

#[derive(Subcommand)]
enum AppCommand {
    /// Launch an app
    Launch {
        /// Package name
        package: String,
    },
    /// Kill an app
    Kill {
        /// Package name
        package: String,
    },
    /// Show current foreground activity
    Active,
    /// List installed packages
    List {
        /// Filter by keyword
        filter: Option<String>,
    },
    /// Install an APK
    Install {
        /// Path to APK file
        path: String,
    },
    /// Clear app data
    Clear {
        /// Package name
        package: String,
    },
}

fn resolve(dev_name: Option<&str>) -> Result<Option<crate::registry::Device>, String> {
    let devices = Registry::load()?;
    if devices.is_empty() && dev_name.is_none() {
        return Ok(None);
    }
    let (_, dev) = Registry::resolve(dev_name, &devices)?;
    Ok(Some(dev))
}

pub fn run(dev_name: Option<&str>, args: AppArgs) -> Result<(), String> {
    match args.command {
        AppCommand::Launch { package } => {
            let dev = resolve(dev_name)?;
            adb::shell(
                dev.as_ref(),
                &[
                    "monkey",
                    "-p",
                    &package,
                    "-c",
                    "android.intent.category.LAUNCHER",
                    "1",
                ],
            )?;
            println!("launched {package}");
            Ok(())
        }
        AppCommand::Kill { package } => {
            let dev = resolve(dev_name)?;
            adb::shell(dev.as_ref(), &["am", "force-stop", &package])?;
            println!("killed {package}");
            Ok(())
        }
        AppCommand::Active => {
            let dev = resolve(dev_name)?;
            let out = adb::shell(dev.as_ref(), &["dumpsys", "window"])?;
            for line in out.lines() {
                let trimmed = line.trim();
                if trimmed.contains("mCurrentFocus") || trimmed.contains("mFocusedApp") {
                    println!("{trimmed}");
                }
            }
            Ok(())
        }
        AppCommand::List { filter } => {
            let dev = resolve(dev_name)?;
            let out = adb::shell(dev.as_ref(), &["pm", "list", "packages"])?;
            for line in out.lines() {
                let pkg = line.trim().strip_prefix("package:").unwrap_or(line.trim());
                if let Some(ref f) = filter {
                    if pkg.contains(f.as_str()) {
                        println!("{pkg}");
                    }
                } else {
                    println!("{pkg}");
                }
            }
            Ok(())
        }
        AppCommand::Install { path } => {
            let dev = resolve(dev_name)?;
            let out = adb::adb(dev.as_ref(), &["install", "-r", &path])?;
            print!("{out}");
            Ok(())
        }
        AppCommand::Clear { package } => {
            let dev = resolve(dev_name)?;
            adb::shell(dev.as_ref(), &["pm", "clear", &package])?;
            println!("cleared {package}");
            Ok(())
        }
    }
}
