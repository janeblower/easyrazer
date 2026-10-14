<p align="center"><img src="logo.svg" width="192" alt=""></p>

<h1 align="center">EasyRazer</h1>

<p align="center">
  A lightweight Razer Synapse replacement for Razer analog keyboards on Windows.
</p>

<p align="center">
  <a href="https://github.com/janeblower/easyrazer/releases/latest"><img src="https://img.shields.io/github/v/release/janeblower/easyrazer" alt="Release"></a>
  <a href="https://github.com/janeblower/easyrazer/actions/workflows/release.yml"><img src="https://github.com/janeblower/easyrazer/actions/workflows/release.yml/badge.svg" alt="Build"></a>
  <a href="https://github.com/janeblower/easyrazer/releases"><img src="https://img.shields.io/github/downloads/janeblower/easyrazer/total" alt="Downloads"></a>
  <img src="https://img.shields.io/badge/platform-Windows-0078D6" alt="Windows">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/janeblower/easyrazer" alt="License"></a>
</p>

<p align="center">
  <a href="https://github.com/janeblower/easyrazer/releases/latest/download/EasyRazer.exe"><b>Download</b></a> ·
  <a href="docs/en/usage.md">User guide</a> ·
  <a href="docs/ru/README.md">Русский</a>
</p>

![Keys tab: actuation points and Rapid Trigger](docs/screenshots/actuation.png)

> [!NOTE]
> Unofficial project, not affiliated with Razer Inc.

## Why EasyRazer

- **Tiny.** One 9 MB `.exe` and one process, ~31 MB of RAM in the tray. Synapse 4 is
  ~1.2 GB on disk, 13 processes and ~1.75 GB of RAM.
- **Settings live in the keyboard.** Actuation points and profiles are written to the
  keyboard's memory and keep working with no software running at all.
- **The firmware types, not the host.** By default the keyboard detects presses itself;
  the host steps in only where the firmware cannot, such as Rapid Trigger on the V2 Analog.
- **Offline.** No account, no network, no telemetry.

Full comparison and the reasoning behind it: [how it works](docs/en/how-it-works.md).

## Features

**Actuation**
- per-key actuation point: "Apply" lasts until reconnect, "Write" stores it in the keyboard;
- per-key Rapid Trigger with separate press and release sensitivity;
- Snap Tap: key groups of any size, a rule per group, Fn+LShift toggle.

**Keys**
- remapping: another key with modifiers, mouse buttons, media, disable;
- macros: keys, mouse, delays, recording;
- Hypershift layer: an action and a press point per Fn combination.

**Profiles**
- any number in the app, up to five in the keyboard; Fn+Menu cycles them.

**Lighting**
- built-in effects with live preview;
- custom per-key and per-zone colour layout;
- Windows Dynamic Lighting toggle.

**System**
- lives in the tray, starts with Windows;
- restores settings after reconnect, sleep and Synapse exit;
- English and Russian UI.

Details and caveats: [user guide](docs/en/usage.md).

## Supported keyboards

| Model | Status |
|---|---|
| Huntsman V2 Analog (`1532:0266`) | ✅ full support |
| Huntsman Mini Analog (`1532:0282`) | 🧪 as V2 Analog, without macros; not tested on hardware |

<details>
<summary>In development, not tested on hardware</summary>

| Model | VID:PID |
|---|---|
| Huntsman V3 Pro | `1532:02A6`, 8 kHz `1532:02CF` |
| Huntsman V3 Pro TKL | `1532:02A7`, 8 kHz `1532:02D0` |
| Huntsman V3 Pro Mini | `1532:02B0` |
| Huntsman V3 TKL 8KHz | `1532:02E5` |
| Huntsman Signature Edition | `1532:02D8` |
| Huntsman V3 Pro Low-profile TKL 8KHz | `1532:02E6` |

</details>

Features per model and caveats: [full table](docs/en/supported.md). Own one of the
untested models? Reports are very welcome.

## Installation

1. Download [`EasyRazer.exe`](https://github.com/janeblower/easyrazer/releases/latest/download/EasyRazer.exe)
   from the latest release. It is a single file with no installer; put it anywhere.
2. Quit Synapse completely, including its tray icon.
3. Run `EasyRazer.exe`.

Nightly builds from `master` are published as the
[`nightly`](https://github.com/janeblower/easyrazer/releases/tag/nightly) pre-release.

## FAQ

**Do I have to uninstall Synapse?**
No, but it must not run at the same time: Synapse keeps the keyboard in its own mode and
overwrites settings. While it runs, EasyRazer writes nothing and puts the keyboard back
into HW mode.

**Does EasyRazer have to keep running?**
Actuation points and profiles written with "Write" live in the keyboard. Rapid Trigger,
Snap Tap and the custom colour layout are computed by the app and need it running.

**Will it wear out my keyboard?**
Changes go to the keyboard's RAM; flash is written only on an explicit "Write", and only
the keys that changed. Before the first write of a session a backup goes to
`%APPDATA%\EasyRazer`. See [flash wear](docs/en/how-it-works.md#flash-wear).

**Why doesn't Rapid Trigger work in admin windows or on the lock screen?**
Windows drops input injected by ordinary apps there, so EasyRazer hands typing back to
the firmware while such a window has focus. The V2 Analog line has no hardware Rapid
Trigger; the V3 line keeps it there. See
[admin windows](docs/en/how-it-works.md#admin-windows-lock-screen-uac).

**Why is the actuation range narrower in HW mode?**
On the V2 Analog line the firmware clamps it to 1.62–3.56 mm; the full 1.5–3.6 mm needs
driver mode. The V3 line has no such limit: 0.1–4.0 mm in both modes.

## Building from source

Requires Rust (MSVC), Bun and MSVC Build Tools.

```
cd app
bun install
bun tauri dev      # run with rebuild on changes
bun tauri build    # ..\target\release\easyrazer.exe
```

Tests (from the repository root): `cargo test`; frontend: `bun run lint` (in `app`).
Test on a real keyboard (Synapse closed): `cargo test -p easyrazer -- --ignored`.

## Documentation

- [User guide](docs/en/usage.md) — features, controls, limitations.
- [How it works](docs/en/how-it-works.md) — comparison with Synapse, modes, flash wear.
- [Development](docs/en/development.md) — layout, rules, adding a keyboard model.
- [Keyboard models](docs/en/devices/README.md) — protocol and firmware of each supported or studied model.

## Roadmap

- [ ] Host-driven lighting animations
- [ ] A proper macro editor (the current one works but is a rough draft)
- [ ] Game mode
- [ ] Gamepad emulation — unlikely; [open an issue](https://github.com/janeblower/easyrazer/issues) if you need it
- [ ] Hardware-tested support for more keyboard models
- [ ] Linux

What has shipped: [releases](https://github.com/janeblower/easyrazer/releases).

## License

[GPL-3.0-or-later](LICENSE).

Razer and Huntsman are trademarks of Razer Inc. This project is not affiliated with,
endorsed by, or sponsored by Razer Inc.
