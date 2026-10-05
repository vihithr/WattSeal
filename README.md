<div align="center">

<img src="resources/svg/banner.svg" alt="WattSeal, Real-time PC power consumption monitoring" width="100%"/>

WattSeal shows you a live breakdown of power consumption of your PC, by component and by app. Monitor which hardware is drawing the most energy, which apps are the biggest energy hogs, and how your usage changes over time.

Available in English, Chinese, French, German and Romanian.

[![Windows](https://img.shields.io/badge/Windows-x86__64-0078D4?style=flat-square&logo=windows)](https://github.com/daminoup88/wattseal/releases)
[![Linux](https://img.shields.io/badge/Linux-x86__64-FCC624?style=flat-square&logo=linux&logoColor=black)](https://github.com/daminoup88/wattseal/releases)
[![macOS](https://img.shields.io/badge/macOS-aarch64-000000?style=flat-square&logo=apple)](https://github.com/daminoup88/wattseal/releases)
[![GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue?style=flat-square)](LICENSE)

<img src="resources/dashboard.png" alt="WattSeal app dashboard showing real-time power consumption breakdown by application and component" width="80%" style="border-radius: 12px; box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1); margin-top: 20px;"/>

</div>

---

## Why use WattSeal?

Most people have no idea how much electricity their computer actually uses, or which apps are silently draining power in the background. WattSeal gives you that visibility:

- 🔍 **Live dashboard**: watch power draw update every second
- 📌 **Overlay widget**: keep the numbers on top of any window, including full-screen games
- 🧩 **Per-component breakdown**: CPU, GPU, RAM, storage, network
- 📋 **Per-app breakdown**: find out which processes are costing you the most
- 📈 **Historical charts**: spot trends over time
- 💾 **Local database**: all your data stays on your machine, private

> Power readings are validated against real hardware measurements using a [Shelly Plug Gen3 S](https://www.shelly.com/products/shelly-plug-s-gen3) smart plug.

---

## Getting Started

### Step 1 — Download

Grab the latest release for your operating system from the **[Releases page](https://github.com/daminoup88/wattseal/releases)**:

| Your system | File to download |
|---|---|
| Windows (64-bit) | `WattSeal-windows.exe` |
| Linux (64-bit) | `WattSeal-linux` |
| macOS (Apple Silicon) | `WattSeal-macos` |

WattSeal is a single executable file — no installation needed. Just download it, and you're ready for the next step.

---

### Step 2 — Run it

WattSeal doesn't need administrative privileges to run, but it does need them for precise CPU power measurements. If you skip the admin step, you'll still get power estimates based on CPU usage, but they won't be as accurate.

<details>
<summary><strong>🪟 Windows</strong></summary>

1. Double-click the downloaded `WattSeal-windows-x86_64.exe` file
2. If prompted by Windows Defender SmartScreen, click "More info" and then "Run anyway" to launch the app. This is a standard warning for new apps that haven't yet built up reputation on Windows.
3. If prompted by User Account Control (UAC), and you want the most accurate CPU power readings, click "Yes" to allow it to run with administrator privileges. If you click "No", it will still work but with less precise CPU power estimates.

The app will launch in the system tray in the taskbar and the dashboard will open in a new window. If you close the dashboard, WattSeal will keep running in the background and you can reopen it by clicking the tray icon.

If the app, or more especifically the WinRing0 kernel driver it uses for CPU measurements, is flagged by Windows Defender, it's your responsibility to allow it to run. The app will run without admin privileges, but CPU power readings will be estimates based on usage rather than direct hardware counters. For more info on the security implications of WinRing0, see the [Security](SECURITY.md#winring0-kernel-driver-windows) section of the documentation.

</details>

<details>
<summary><strong>🐧 Linux</strong></summary>

Open a terminal in the folder where you downloaded WattSeal and run:

```bash
chmod +x WattSeal-linux
sudo ./WattSeal-linux
```

> **Note:** the only extra runtime dependency is an X11 system tray library.
> If either `libappindicator` **or** `libayatana-appindicator` is installed
> the app will show a tray icon with menu items; otherwise WattSeal will
> simply run in the background without a tray icon (you can still open the
> dashboard by re‑running the command).

</details>

<details>
<summary><strong>🍎 macOS</strong></summary>

Run the app normally, WattSeal will work without admin privileges.

</details>

---

## What can WattSeal measure?

| Component | How it's measured |
|---|---|
| **CPU (Intel / AMD)** | Direct hardware energy counters (RAPL) — very accurate |
| **GPU (NVIDIA)** | NVML vendor API — very accurate |
| **GPU (AMD, Windows)** | ADLX vendor API — very accurate |
| **GPU (Intel, Windows)** | PDH performance counters |
| **RAM** | Estimated from memory usage |
| **Disk** | Estimated from read/write activity |
| **Network** | Estimated from data throughput |
| **Per-process** | CPU + GPU + I/O breakdown per app |

> **What does "estimated" mean?** For components without built-in energy sensors, WattSeal calculates a best-guess power draw based on how hard the hardware is working and its known power specs. It's less precise than hardware counters, but still gives a solid picture.

---

## Overlay widget

> **This widget is a fork addition, not part of upstream WattSeal.** It is developed here, in this
> repository, and is not maintained by the upstream project. Please report anything about it —
> including crashes that happen only with the overlay running — **to this repository's issue
> tracker**, not upstream's.

**WattSeal Overlay** is a separate small program that shows live power metrics in a compact
**always-on-top** window, so you can keep an eye on power draw without leaving the dashboard open —
including on top of a full-screen game.

It is a **standalone program**, not a feature of this build. That is the point: it runs against the
**official WattSeal**, it links none of WattSeal's code, and an upstream release never needs a
rebuild here. Unpack it next to WattSeal and start `wattseal-overlay` — if WattSeal is not already
running, the widget starts it.

| | |
|---|---|
| **Start it** | Run `wattseal-overlay` — it starts WattSeal for you when there is nothing to read, so one icon is enough |
| **Move it** | Drag it with the left mouse button |
| **Menu** | Right-click for `Settings` / `Pin` / `Hide overlay` |
| **Pin it** | Locks the position, and on Windows also makes the widget ignore the mouse so your clicks reach the app underneath |
| **Style it** | Vertical or horizontal layout, three densities, three text sizes, background and text colors, opacity, drop shadow |
| **Trim it** | Choose which metrics appear and in which order (total, CPU, GPU, RAM, disk, network, top apps), shorten the labels, set the decimals and the refresh interval |
| **Language and theme** | Follow the dashboard by default; pick your own in the settings panel |

The widget hugs its numbers — the height always follows the content, and the width is whatever the
numbers need, capped by the `Width` setting. It never wraps onto a second line, and it never cuts a
reading in half: if the cap is too narrow, it gives up labels first and whole metrics last. A pinned
widget is deliberately not draggable, and with click-through on it does not take the mouse either:
there is then no menu and no settings panel, so **restarting does not release it**. Two ways out:
**hold `Ctrl` and `Alt`** — the widget stops ignoring the mouse while you hold them, and its menu keeps
the mouse until it closes, so the gesture is hold, right-click, let go, click — or press the shortcut,
`Ctrl+Alt+O` unless you bind another in the settings panel. The held keys need no shortcut registration,
so unlike the shortcut they cannot be taken by another program. While it is stuck, the widget says so on
its own card, because reading it still works when clicking it does not. Only one overlay runs at a time,
so starting a second leaves the first one alone.

The widget reads WattSeal's database **read-only**, by column name, and checks the schema
generation before reading anything: if the collector has moved on to a generation this build does
not know, it shows the readings it can still confirm and hides the rest rather than guessing.

It is a **reader**, never a writer — not of the database, which it opens with
`SQLITE_OPEN_READ_ONLY`, and not of anything else. If the overlay breaks, the dashboard is
unaffected, and the numbers it shows are WattSeal's own, taken from the same database the dashboard
uses.

When there is nothing to show, the widget says **which** thing is missing rather than sitting there
blank: WattSeal not found, still starting, stopped writing, launching switched off, or a database
newer than this build. A stopped collector never shows its readings — a frozen number looks exactly
like a live one.

See [doc/overlay.md](doc/overlay.md) for the transparency model, the settings panel and the full
config file reference.

The release build opens no console window. If it ever fails to start, the reason is in
`startup_error.txt` next to the executable.

---

## Platform Support

With admin privileges, WattSeal provides the most comprehensive power monitoring experience possible on each platform:

|                                                      | Windows |      Linux      |   macOS   |
|------------------------------------------------------|:-------:|:---------------:|:---------:|
| Full application                                     |    ✅    |        ✅        |     ✅     |
| CPU energy counters                                  |    ✅    |        ✅        | Estimated |
| NVIDIA GPU                                           |    ✅    |        ✅        |     ❌     |
| AMD GPU                                              |    ✅    |        ❌        |     ❌     |
| Intel GPU                                            |    ✅    |        ❌        |     ❌     |
| Other sensors (usage, I/O)                           |    ✅    |        ✅        |     ✅     |
| Auto admin elevation                                 |  ✅ UAC  | Manual (`sudo`) |  Manual   |
| Auto start on login                                  |    ✅    |        ❌        |     ❌     |
| Overlay widget                                       |    ✅    |        ✅        |     ✅     |
| Overlay mouse pass-through (pin)                     |    ✅    |        ❌        |     ❌     |

> Overlay transparency uses a layered window on Windows and per-pixel surface alpha elsewhere; mouse pass-through is Windows-only. See [Overlay widget](#overlay-widget).

<details>
<summary><strong>Support without admin privileges</strong></summary>

|                            |  Windows  |   Linux   |   macOS   |
|----------------------------|:---------:|:---------:|:---------:|
| Full application           |     ✅     |     ✅     |     ✅     |
| CPU energy counters        | Estimated | Estimated | Estimated |
| NVIDIA GPU                 |     ✅     |     ✅     |     ❌     |
| AMD GPU                    |     ✅     |     ❌     |     ❌     |
| Intel GPU                  |     ✅     |     ❌     |     ❌     |
| Other sensors (usage, I/O) |     ✅     |     ✅     |     ✅     |

</details>

---

## Language Support

The following languages are supported by WattSeal:

| Language             | Status            | Contributor                                       |
|----------------------|-------------------|---------------------------------------------------|
| Chinese (Simplified) | ✅ Fully supported | [metahubaifeel](https://github.com/metahubaifeel) |
| English              | ✅ Fully supported | [Daminoup88](https://github.com/Daminoup88)       |
| French               | ✅ Fully supported | [Daminoup88](https://github.com/Daminoup88)       |
| Romanian             | ✅ Fully supported | [CodrinSocol](https://github.com/CodrinSocol)     |
| German               | ✅ Fully supported | [LivingPixel-pixel](https://github.com/LivingPixel-pixel)|


## Troubleshooting

**Rendering issues?** If the UI looks broken or fails to launch, try setting the environment variable `ICED_BACKEND=tiny-skia` before running the app. This forces Iced to use a software renderer which is more compatible with older GPUs and VMs.

**Overlay window stays opaque?** Transparency depends on what the GPU exposes: some surfaces offer only an opaque composite mode, in which case the overlay falls back to a layered window. Run it with `WATTSEAL_OVERLAY_LOG=1` to record the selected adapter and the alpha modes that were available — see [doc/overlay.md](doc/overlay.md).

# 🛠️ Developer Documentation
<div align="center">

[![Built with Rust](https://img.shields.io/badge/Built%20With-Rust-CE422B?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![Built with Iced](https://img.shields.io/badge/Built%20With%20Iced-3645FF?logo=iced&logoColor=fff)]()

</div>

The rest of this README is aimed at contributors and developers who want to build WattSeal from source, understand its architecture, or add new features.

> Want to contribute? Check out our [CONTRIBUTING.md](CONTRIBUTING.md) and our [ROADMAP.md](ROADMAP.md) for planned features and areas where help is needed.

---

## Architecture Overview

WattSeal is a Rust workspace. The `wattseal` binary is the entry point, and each of the other
crates has a single responsibility:

```
wattseal/               ← Root binary (tray icon, lifecycle management)
  ├── collector/        ← Background sensor polling, power estimation, DB writes
  ├── common/           ← Shared types, SQLite layer, utilities
  ├── ui/               ← Iced GUI (dashboard, hardware info, settings, charts)
  ├── overlay/          ← Standalone metrics widget — its own program, its own workspace
  └── mqtt/             ← Publishes collected data to an MQTT broker
```

**How the pieces fit together:**

![Architecture diagram](resources/svg/overall_architecture.svg)

The collector and UI share the same SQLite database file via WAL (Write-Ahead Logging) mode, which allows concurrent reads and writes without locking.

---

## Prerequisites

- **Rust** stable toolchain (version pinned in [`rust-toolchain.toml`](rust-toolchain.toml)).
- On linux, install the build deps for the tray icon, not needed at runtime but required to build the Linux version:

  ```bash
  sudo apt install libgtk-3-dev pkg-config libxkbcommon-dev libwayland-dev
  ```

---

## Building from Source

Clone the repository:
```bash
git clone https://github.com/daminoup88/wattseal.git
```

```bash
cd wattseal
```

Debug build and run:
```bash
cargo run
```

Release build:
```bash
cargo build --release
```

> ⚠️ **Elevated privileges are required** to access hardware energy counters.
> Run with administrator rights on Windows (you will be prompted to elevate), or use `sudo` on Linux.

---

## Project Layout

| Path          | What it does                                                              |
|---------------|---------------------------------------------------------------------------|
| `src/main.rs` | Entry point: admin elevation, tray icon, collector thread, UI subprocess  |
| `collector/`  | All sensor implementations (CPU, GPU, RAM, disk, network, per-process)    |
| `common/`     | Shared types (`Event`, `SensorData`, …), SQLite database layer, utilities |
| `ui/`         | Iced application: pages, components, charts, themes, translations         |
| `overlay/`    | Standalone always-on-top widget — **its own workspace**, reads the database and links nothing above |
| `mqtt/`       | MQTT publishing of collected sensor data and hardware info                |

---

## Code Style & Quality

The project enforces the formatting and linting rules defined in `rustfmt.toml`. Compliance is checked in CI. You can run the following command locally to ensure your code meets the project's style guidelines before pushing:

```bash
cargo +nightly fmt
```

> The `.vscode/settings.json` and `.zed/settings.json` are configured to format on save, so if you're using VS Code or Zed your code will be formatted automatically when you save a file.

---

# License

WattSeal is licensed under [GPL-3.0](LICENSE). See the [LICENSE](LICENSE) file for details.
