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
    /// Install an APK and launch the app
    Deploy {
        /// Path to APK file
        path: String,
        /// Package name (inferred from APK if omitted)
        package: Option<String>,
    },
}

fn find_aapt2() -> Option<std::path::PathBuf> {
    if std::process::Command::new("aapt2").arg("version").output().is_ok() {
        return Some(std::path::PathBuf::from("aapt2"));
    }
    let home = std::env::var("ANDROID_HOME")
        .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
        .ok()?;
    let bt = std::path::Path::new(&home).join("build-tools");
    let mut versions: Vec<_> = std::fs::read_dir(&bt).ok()?.filter_map(|e| e.ok()).collect();
    versions.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    versions.into_iter().find_map(|v| {
        let p = v.path().join("aapt2");
        p.exists().then_some(p)
    })
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
        AppCommand::Deploy { path, package } => {
            let dev = resolve(dev_name)?;
            let pkg = match package {
                Some(p) => p,
                None => {
                    let aapt2 = find_aapt2().ok_or("aapt2 not found: not on PATH and ANDROID_HOME not set")?;
                    let out = std::process::Command::new(&aapt2)
                        .args(["dump", "badging", &path])
                        .output()
                        .map_err(|e| format!("failed to run {}: {e}", aapt2.display()))?;
                    if !out.status.success() {
                        return Err(format!("aapt2 failed: {}", String::from_utf8_lossy(&out.stderr)));
                    }
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    stdout
                        .lines()
                        .find(|l| l.starts_with("package:"))
                        .and_then(|l| {
                            l.split_whitespace()
                                .find(|t| t.starts_with("name='"))
                                .map(|t| t.trim_start_matches("name='").trim_end_matches('\'').to_string())
                        })
                        .ok_or_else(|| "could not extract package name from APK".to_string())?
                }
            };
            let out = adb::adb(dev.as_ref(), &["install", "-r", &path])?;
            print!("{out}");
            adb::shell(
                dev.as_ref(),
                &["monkey", "-p", &pkg, "-c", "android.intent.category.LAUNCHER", "1"],
            )?;
            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(300));
                let focus = adb::shell(dev.as_ref(), &["dumpsys", "window"])?;
                if focus.lines().any(|l| l.contains("mCurrentFocus") && l.contains(&pkg)) {
                    println!("launched {pkg}");
                    return Ok(());
                }
            }
            println!("launched {pkg} (activity not yet focused after 3s)");
            Ok(())
        }
    }
}
