# ddb — Device Debug Bridge

Unified Android device CLI that replaces raw adb shell scripts, uiautomator wrappers, and scrcpy launchers. Manages device enrollment, wireless reconnection, UI inspection, semantic extraction, screenshot capture, scroll-capture composites, interactive test specs, and app lifecycle — all through a single binary with automatic device serial injection.

## Install

```bash
cd device-control-android/ddb
cargo build --release
cp target/release/ddb /opt/homebrew/bin/ddb
```

Requires `adb` on PATH. Optional: `scrcpy` for screen mirroring, `aapt2` for APK package detection.

## Global Flags

| Flag | Description |
|------|-------------|
| `-d, --device <name>` | Target device by enrolled name. Auto-selects if only one device is enrolled. |

## Commands

### devices

Manage enrolled Android devices and wireless connections.

```bash
ddb devices list                    # list all enrolled devices
ddb devices status [name]           # connection status (WiFi/USB/ping/daemon)
ddb devices add <name> --serial <SN> --model <M> --android <V> --sdk <N> [--wifi-ip <IP>] [--adb-port <PORT>] [--force]
ddb devices remove <name>           # remove from registry
ddb devices connect <name>          # establish wireless ADB
ddb devices disconnect <name>       # drop wireless ADB
```

**Example:**
```bash
ddb devices add pixel --serial ABCDEF --model "Pixel 8" --android 15 --sdk 35 --wifi-ip 192.168.1.42
ddb devices connect pixel
ddb devices status pixel
```

### tap

Tap at screen coordinates.

```bash
ddb tap <x> <y>
```

### swipe

Swipe between two points.

```bash
ddb swipe <x1> <y1> <x2> <y2> [--duration <ms>]
```

Duration default: 300ms.

### scroll

Scroll in a cardinal direction. Swipes through screen center with 500ms duration.

```bash
ddb scroll <up|down|left|right>
```

### type

Type text into the focused field. Spaces are handled automatically.

```bash
ddb type "hello world"
```

### button

Press a hardware or system button.

```bash
ddb button <name>
```

Names: `home`, `back`, `power`, `enter`, `menu`, `recents`, `volup`/`volume_up`, `voldown`/`volume_down`, `del`/`delete`, or a raw Android keycode.

### home

Shorthand for `ddb button home`.

### back

Shorthand for `ddb button back`.

### ui

Dump the current UI hierarchy. Default output uses markers: `●` = clickable, `○` = label only.

```bash
ddb ui                              # compact element list with tap coordinates
ddb ui --raw                        # raw uiautomator XML
ddb ui --json                       # JSON array of elements
ddb ui --semantic -o /tmp/ui.yaml   # semantic YAML via agent (falls back to uiautomator)
ddb ui --semantic --no-agent        # force uiautomator, skip agent
ddb ui --semantic --source-root ./  # resolve resource IDs from source tree
```

| Flag | Description |
|------|-------------|
| `--raw` | Raw uiautomator XML dump |
| `--json` | JSON-formatted elements |
| `--semantic` | Produce semantic schema YAML |
| `--no-agent` | Skip semantic agent auto-detect, use uiautomator |
| `--source-root <path>` | Android source root for resource resolution |
| `-o, --output <path>` | Output file (default: stdout) |

**Semantic agent:** When `--semantic` is used, ddb first tries `curl http://localhost:9876/semantic` to get agent-provided data. If unavailable, falls back to uiautomator parsing.

### screenshot

Capture and auto-downscale a screenshot.

```bash
ddb screenshot [output]             # default: /tmp/screen.png
ddb screenshot -d pixel /tmp/s.png  # specific device
```

### scroll-capture

Scroll through a screen and stitch frames into a single composite image.

```bash
ddb scroll-capture [-o /tmp/composite.png] [--scroll-pct 20] [--max-steps 30] [--keep-steps]
```

| Flag | Description |
|------|-------------|
| `-o, --output <path>` | Output composite PNG (default: `/tmp/android-scroll-composite.png`) |
| `--scroll-pct <n>` | Scroll percentage per step (default: 20, smaller = more overlap) |
| `--max-steps <n>` | Max scroll steps (default: 30) |
| `--keep-steps` | Save individual frame PNGs |

Algorithm: scrolls to top, captures frames, detects sticky headers/footers, uses image correlation for overlap detection, stitches into final composite.

### app

App lifecycle management.

```bash
ddb app launch <package>            # launch via monkey
ddb app kill <package>              # force-stop
ddb app active                      # show foreground activity
ddb app list [filter]               # list packages (optional keyword filter)
ddb app install <path.apk>          # install APK (-r flag)
ddb app clear <package>             # clear app data
ddb app deploy <path.apk> [package] # install + launch + wait for focus
```

`deploy` infers the package name from the APK via aapt2 if not provided.

### mount

Inject debug instrumentation into an Android project.

```bash
ddb mount semantic [--project <path>]
```

Copies semantic agent source files to `app/src/debug/java/dev/substrate/semantic/`, adds debug AndroidManifest with ContentProvider registration, creates Gradle init script with nanohttpd dependency. Requires rebuild after mount.

### unmount

Remove injected debug instrumentation.

```bash
ddb unmount semantic [--project <path>]
```

Removes agent source files and Gradle init script. Requires rebuild after unmount.

### test

Run interactive test specifications against the device.

```bash
ddb test spec.yaml [--report /tmp/results.json] [--step-timeout 10]
```

| Flag | Description |
|------|-------------|
| `--report <path>` | Output JSON test report |
| `--step-timeout <secs>` | Timeout per step (default: 10) |

**Test spec format (YAML):**
```yaml
id: login_flow
name: "Login flow test"
precondition:
  activity: "com.example.MainActivity"
  scroll_to: "login_button"
steps:
  - action: tap
    target: { id: "email_field" }
  - action: type
    text: "user@example.com"
  - action: tap
    target: { text: "Sign In" }
  - assert: activity
    expected: "com.example.HomeActivity"
  - assert: element_exists
    target: { id: "welcome_text" }
    text: "Welcome"
```

Actions: `tap`, `type`, `scroll`, `scroll_to`, `back`, `home`. Assertions: `activity`, `element_exists`, `element_state`. Disables animations during test. Captures screenshot on failure.

### mirror

Launch scrcpy for screen mirroring.

```bash
ddb mirror [-- --max-fps 30]
```

Extra scrcpy arguments go after `--`. Uses `config.scrcpy_path` from config.toml. Always enables `--legacy-paste`.

### daemon

Manage wireless ADB heartbeat daemons (macOS launchd).

```bash
ddb daemon start <name>             # start auto-reconnect daemon
ddb daemon stop <name>              # stop daemon
ddb daemon status [name]            # check daemon state
ddb daemon log <name>               # tail heartbeat log
```

Checks ping every 10s, auto-reconnects after 3 consecutive misses.

### config

Manage tool configuration.

```bash
ddb config init                     # create default config + registry
ddb config show                     # print current config
ddb config set <key> <value>        # set: adb_path, scrcpy_path, default_device
ddb config path                     # print config file path
```

### doctor

Verify prerequisites and diagnose issues.

```bash
ddb doctor
```

Checks: adb version, scrcpy availability, config validity, device connections.

### adb

Pass through to adb with automatic `-s <serial>` injection from device registry.

```bash
ddb adb logcat -s MyTag
ddb adb push local.txt /sdcard/
ddb adb pull /sdcard/data.json .
ddb adb shell getprop ro.product.model
```

### completions

Generate shell completion scripts.

```bash
ddb completions <bash|zsh|fish|elvish|powershell>
```

## Configuration

| File | Location | Purpose |
|------|----------|---------|
| Config | `~/.config/ddb/config.toml` | adb_path, scrcpy_path, default_device |
| Registry | `~/.config/ddb/devices.toml` | Enrolled devices (serial, model, WiFi IP, SDK level) |
| Heartbeat logs | `/tmp/ddb-<name>-heartbeat.log` | Daemon connection logs |
| Launchd plists | `~/Library/LaunchAgents/com.user.ddb-heartbeat-<name>.plist` | macOS daemon configs |

## Examples

**Full QA workflow:**
```bash
ddb devices connect a54
ddb app deploy /tmp/app-debug.apk se.naturkartan.android
ddb ui --semantic -o /tmp/android-home.yaml
ddb screenshot /tmp/home.png
ddb scroll-capture -o /tmp/home-full.png
```

**Semantic extraction pipeline:**
```bash
ddb mount semantic --project ./my-app
cd my-app && ./gradlew assembleStandardDebug
ddb app deploy /tmp/app-debug.apk
ddb ui --semantic -o /tmp/semantic.yaml
```

**Test runner:**
```bash
ddb test tests/login.yaml tests/search.yaml --report /tmp/qa-results.json
```
