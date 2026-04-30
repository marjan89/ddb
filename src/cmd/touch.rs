use clap::Args;

use crate::adb;
use crate::registry::Registry;

// Android keycodes — https://developer.android.com/reference/android/view/KeyEvent
mod keycode {
    pub const HOME: &str = "3";
    pub const BACK: &str = "4";
    pub const VOLUME_UP: &str = "24";
    pub const VOLUME_DOWN: &str = "25";
    pub const POWER: &str = "26";
    pub const ENTER: &str = "66";
    pub const DEL: &str = "67";
    pub const MENU: &str = "82";
    pub const APP_SWITCH: &str = "187";
}

// Scroll geometry — assumes 1080-wide screen, swipe through center
const SCROLL_CENTER_X: i32 = 540;
const SCROLL_NEAR_TOP: i32 = 800;
const SCROLL_NEAR_BOTTOM: i32 = 1400;
const SCROLL_CENTER_Y: i32 = 1000;
const SCROLL_NEAR_LEFT: i32 = 200;
const SCROLL_NEAR_RIGHT: i32 = 800;
const SCROLL_DURATION_MS: &str = "500";

#[derive(Args)]
pub struct TapArgs {
    pub x: i32,
    pub y: i32,
}

#[derive(Args)]
pub struct SwipeArgs {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    /// Duration in milliseconds
    #[arg(long, default_value = "300")]
    pub duration: u32,
}

#[derive(Args)]
pub struct TypeArgs {
    /// Text to type (spaces become %s)
    pub text: String,
}

#[derive(Args)]
pub struct ButtonArgs {
    /// Button name: home, back, enter, menu, power, recents
    pub name: String,
}

#[derive(Args)]
pub struct ScrollArgs {
    /// Direction: up, down, left, right
    pub direction: String,
}

fn resolve_device(name: Option<&str>) -> Result<Option<crate::registry::Device>, String> {
    let devices = Registry::load()?;
    if devices.is_empty() && name.is_none() {
        return Ok(None);
    }
    let (_, dev) = Registry::resolve(name, &devices)?;
    Ok(Some(dev))
}

pub fn tap(dev_name: Option<&str>, args: TapArgs) -> Result<(), String> {
    let dev = resolve_device(dev_name)?;
    adb::shell(
        dev.as_ref(),
        &["input", "tap", &args.x.to_string(), &args.y.to_string()],
    )?;
    Ok(())
}

pub fn swipe(dev_name: Option<&str>, args: SwipeArgs) -> Result<(), String> {
    let dev = resolve_device(dev_name)?;
    adb::shell(
        dev.as_ref(),
        &[
            "input",
            "swipe",
            &args.x1.to_string(),
            &args.y1.to_string(),
            &args.x2.to_string(),
            &args.y2.to_string(),
            &args.duration.to_string(),
        ],
    )?;
    Ok(())
}

pub fn type_text(dev_name: Option<&str>, args: TypeArgs) -> Result<(), String> {
    let dev = resolve_device(dev_name)?;
    let escaped = args.text.replace(' ', "%s");
    adb::shell(dev.as_ref(), &["input", "text", &escaped])?;
    Ok(())
}

pub fn button(dev_name: Option<&str>, args: ButtonArgs) -> Result<(), String> {
    let dev = resolve_device(dev_name)?;
    let lowered = args.name.to_lowercase();
    let code = match lowered.as_str() {
        "home" => keycode::HOME,
        "back" => keycode::BACK,
        "power" => keycode::POWER,
        "enter" => keycode::ENTER,
        "menu" => keycode::MENU,
        "recents" => keycode::APP_SWITCH,
        "volume_up" | "volup" => keycode::VOLUME_UP,
        "volume_down" | "voldown" => keycode::VOLUME_DOWN,
        "delete" | "del" => keycode::DEL,
        other => other, // pass raw keycode
    };
    adb::shell(dev.as_ref(), &["input", "keyevent", code])?;
    Ok(())
}

pub fn home(dev_name: Option<&str>) -> Result<(), String> {
    let dev = resolve_device(dev_name)?;
    adb::shell(dev.as_ref(), &["input", "keyevent", keycode::HOME])?;
    Ok(())
}

pub fn back(dev_name: Option<&str>) -> Result<(), String> {
    let dev = resolve_device(dev_name)?;
    adb::shell(dev.as_ref(), &["input", "keyevent", keycode::BACK])?;
    Ok(())
}

pub fn scroll(dev_name: Option<&str>, args: ScrollArgs) -> Result<(), String> {
    let dev = resolve_device(dev_name)?;
    let lowered = args.direction.to_lowercase();
    let (x1, y1, x2, y2) = match lowered.as_str() {
        "down" => (
            SCROLL_CENTER_X,
            SCROLL_NEAR_BOTTOM,
            SCROLL_CENTER_X,
            SCROLL_NEAR_TOP,
        ),
        "up" => (
            SCROLL_CENTER_X,
            SCROLL_NEAR_TOP,
            SCROLL_CENTER_X,
            SCROLL_NEAR_BOTTOM,
        ),
        "left" => (
            SCROLL_NEAR_RIGHT,
            SCROLL_CENTER_Y,
            SCROLL_NEAR_LEFT,
            SCROLL_CENTER_Y,
        ),
        "right" => (
            SCROLL_NEAR_LEFT,
            SCROLL_CENTER_Y,
            SCROLL_NEAR_RIGHT,
            SCROLL_CENTER_Y,
        ),
        other => {
            return Err(format!(
                "unknown direction '{other}'. use: up, down, left, right"
            ))
        }
    };
    adb::shell(
        dev.as_ref(),
        &[
            "input",
            "swipe",
            &x1.to_string(),
            &y1.to_string(),
            &x2.to_string(),
            &y2.to_string(),
            SCROLL_DURATION_MS,
        ],
    )?;
    Ok(())
}
