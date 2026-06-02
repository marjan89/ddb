mod adb_passthrough;
mod app;
mod config_cmd;
pub mod crawl;
mod daemon;
mod devices;
mod doctor;
mod mirror;
mod mount;
mod screenshot;
mod scroll_capture;
mod test;
pub mod test_element;
pub mod test_fixture;
pub mod test_observability;
pub mod test_log;
pub mod test_timeout;
mod touch;
mod ui;

use clap::{Args, CommandFactory, Parser, Subcommand};

/// Device Debug Bridge — unified Android device CLI
#[derive(Parser)]
#[command(name = "ddb", version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("DDB_GIT_HASH"), ")"), about)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args, Clone)]
pub struct GlobalOpts {
    /// Target device name
    #[arg(short, long, global = true)]
    pub device: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// List, enroll, manage devices
    Devices(devices::DevicesArgs),

    /// Tap at coordinates
    Tap(touch::TapArgs),
    /// Swipe between coordinates
    Swipe(touch::SwipeArgs),
    /// Type text
    Type(touch::TypeArgs),
    /// Press a hardware button
    Button(touch::ButtonArgs),

    /// Press home
    Home,
    /// Swipe back gesture
    Back,
    /// Scroll in a direction
    Scroll(touch::ScrollArgs),

    /// Dump UI hierarchy
    Ui(ui::UiArgs),
    /// Capture screenshot
    Screenshot(screenshot::ScreenshotArgs),

    /// Capture scrolling composite screenshot
    ScrollCapture(scroll_capture::ScrollCaptureArgs),

    /// App management
    App(app::AppArgs),

    /// Screen mirroring via scrcpy
    Mirror(mirror::MirrorArgs),

    /// Heartbeat daemon management
    Daemon(daemon::DaemonArgs),

    /// System health checks
    Doctor,

    /// Configuration
    Config(config_cmd::ConfigArgs),

    /// Run interactive test specs on device
    Test(test::TestArgs),

    /// Inject debug instrumentation into a project
    Mount(mount::MountArgs),
    /// Remove debug instrumentation from a project
    Unmount(mount::MountArgs),

    /// Autonomous app exploration — discovers screens, elements, navigation graph
    Crawl(crawl::CrawlArgs),

    /// Pass through to adb (auto-injects -s from registry)
    Adb(adb_passthrough::AdbArgs),

    /// Generate shell completions
    Completions {
        /// Shell to generate for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

pub fn run(cli: Cli) -> Result<(), String> {
    let dev = cli.global.device.as_deref();
    match cli.command {
        Command::Devices(args) => devices::run(args),
        Command::Tap(args) => touch::tap(dev, args),
        Command::Swipe(args) => touch::swipe(dev, args),
        Command::Type(args) => touch::type_text(dev, args),
        Command::Button(args) => touch::button(dev, args),
        Command::Home => touch::home(dev),
        Command::Back => touch::back(dev),
        Command::Scroll(args) => touch::scroll(dev, args),
        Command::Ui(args) => ui::run(dev, args),
        Command::Screenshot(args) => screenshot::run(dev, args),
        Command::ScrollCapture(args) => scroll_capture::run(dev, args),
        Command::App(args) => app::run(dev, args),
        Command::Mirror(args) => mirror::run(dev, args),
        Command::Daemon(args) => daemon::run(args),
        Command::Doctor => doctor::run(),
        Command::Config(args) => config_cmd::run(args),
        Command::Test(args) => test::run(dev, args),
        Command::Crawl(args) => crawl::run(dev, args),
        Command::Mount(args) => mount::run(args),
        Command::Unmount(args) => mount::unmount(args),
        Command::Adb(args) => adb_passthrough::run(dev, args),
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "ddb",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    }
}
