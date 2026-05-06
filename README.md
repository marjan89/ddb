# ddb — Device Debug Bridge

Unified Android device CLI. One tool replacing `adb` shell scripts, `ui.py`, `scrcpy` wrappers, and launchd heartbeat daemons.

## Install

```bash
cargo build --release
cp target/release/ddb /usr/local/bin/
```

## Setup

```bash
ddb config init                    # creates ~/.config/ddb/config.toml + devices.toml
ddb devices add pixel \
  --serial ABCDEF123456 \
  --model "Pixel 8 Pro" \
  --android 15 --sdk 35 \
  --wifi-ip 192.168.1.42
ddb devices connect pixel          # wireless ADB
```

## Usage

```
ddb [-d <device>] <command>
```

Global `-d` flag selects the target device by name. Auto-selects if only one device is enrolled.

### Device management

```bash
ddb devices list                   # enrolled devices
ddb devices status                 # connection status (wifi/usb/ping/daemon)
ddb devices connect <name>         # wireless ADB (switches from USB if needed)
ddb devices disconnect <name>
ddb devices add <name> --serial ...
ddb devices add <name> --serial ... --force   # re-enroll: overwrites all fields, keeps original `enrolled` date
ddb devices remove <name>
```

Re-enrolling is useful after a factory reset, IP change, or when correcting a typo. Example:

```bash
ddb devices add a54 \
  --serial RZCW60WF36N \
  --model "Samsung Galaxy A54 (SM-A546B)" \
  --android 15 --sdk 35 \
  --wifi-ip 192.168.1.79 \
  --force
```

### Touch & input

```bash
ddb tap 540 1200                   # tap at coordinates
ddb swipe 540 1400 540 800         # swipe (default 300ms)
ddb swipe 540 1400 540 800 --duration 800
ddb type "hello world"             # type text (spaces handled)
ddb button home                    # home, back, enter, menu, power, recents, volup, voldown, del
ddb home                           # shorthand
ddb back                           # shorthand
ddb scroll down                    # up, down, left, right
```

### UI inspection

```bash
ddb ui                             # compact element list
ddb ui --json                      # JSON output
ddb ui --raw                       # raw uiautomator XML
```

Output:
```
● ( 540, 396)  Add your Audio Device
● ( 951, 699)  Connect  [connect_to_device_button]
○ ( 540, 277)  SELECT YOUR HEADPHONES
```

`●` = clickable, `○` = label only. Coordinates are tap-ready.

### Apps

```bash
ddb app launch com.example.app
ddb app kill com.example.app
ddb app active                     # current foreground activity
ddb app list                       # all packages
ddb app list bragi                 # filter by keyword
ddb app install path/to/app.apk
ddb app clear com.example.app      # clear app data
```

### Screenshot

```bash
ddb screenshot                     # saves to /tmp/screen.png, downscaled
ddb screenshot output.png          # custom path
```

### Screen mirroring

```bash
ddb mirror                         # launches scrcpy with --legacy-paste
ddb mirror -- --max-fps 30         # extra scrcpy args
ddb -d a54 mirror                  # by enrolled name
ddb -d RZCW60WF36N mirror          # by raw adb serial (works for unenrolled devices too)
```

Target resolution:

- **No `-d`**: picks the only device currently visible to adb. If more than one is attached, lists candidates and exits.
- **`-d <name>`**: tries the enrolled registry first, then falls back to any matching adb serial. So a freshly plugged-in device can be mirrored without enrolling it first.

Listing format includes both the enrolled name (when available) and the transport id, e.g.:

```
multiple devices available, specify one with -d: a54 (192.168.1.79:5555), mi-a2 (192.168.1.78:5556)
```

### Heartbeat daemon

Auto-reconnects wireless ADB when the connection drops.

```bash
ddb daemon start <name>            # creates + loads launchd plist
ddb daemon stop <name>             # unloads + removes plist
ddb daemon status                  # all devices
ddb daemon log <name>              # tail heartbeat log
```

### ADB passthrough

For anything ddb doesn't have a dedicated command for. Auto-injects `-s` from the device registry.

```bash
ddb adb logcat -s MyTag
ddb -d pixel adb shell getprop ro.product.model
ddb adb push local.txt /sdcard/
```

### System health

```bash
ddb doctor                         # checks adb, scrcpy, config, device connectivity
```

### Configuration

```bash
ddb config init                    # create defaults
ddb config show                    # print current config
ddb config set default_device a54  # set a value
ddb config path                    # print config file path
```

## Config files

| File | Location |
|------|----------|
| Config | `~/.config/ddb/config.toml` |
| Device registry | `~/.config/ddb/devices.toml` |
| Heartbeat logs | `/tmp/ddb-<name>-heartbeat.log` |
| Launchd plists | `~/Library/LaunchAgents/com.user.ddb-heartbeat-<name>.plist` |

## Device registry format

```toml
[pixel]
serial = "ABCDEF123456"
model = "Pixel 8 Pro"
android = "15"
sdk = 35
wifi_ip = "192.168.1.42"
adb_port = 5555
enrolled = "2026-05-01"
```

## Related

- [idb](https://github.com/marjan89/idb) — iOS Device Bridge (Swift)
- [wdb](https://github.com/marjan89/wdb) — Web Debug Bridge (Go)
