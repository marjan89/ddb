mod adb;
mod agent_yaml;
mod catalogue;
mod cmd;
mod config;
mod debug;
mod install_check;
mod registry;
mod semantic;
mod subprocess;
mod ui_parser;

use clap::Parser;
use cmd::Cli;

fn main() {
    install_check::check_binary_mtime();
    let cli = Cli::parse();
    if let Err(e) = cmd::run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
