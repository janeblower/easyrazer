# Development

[Русская версия](../ru/development.md)

## Environment

Windows 11, Rust (`stable-x86_64-pc-windows-msvc`), MSVC Build Tools, Bun.
Building and running — see the [README](../../README.md#building-from-source).

Tests:

```
cargo test                          # core and app, from the root
cargo test -p easyrazer -- --ignored  # on a live keyboard, Synapse closed
cd app && bun run lint              # frontend
```

The core is tested against the keyboard fake `crates/core/src/fake.rs`: it understands
actuation, effect and `0F:03` commands. A new command in the core goes into the fake first.

## Layout

| Path | What is there |
|---|---|
| `crates/core` | Razer protocol with no OS dependencies; `hidapi` behind the `hid` feature |
| `crates/core/devices/*.json` | Model descriptions: PID, control interface, effect set |
| `crates/core/protocols/*.json` | Lighting command sets and effect byte templates |
| `crates/core/src/keymap.rs` | Key names → `fwID` |
| `crates/core/src/layout.rs` | Key geometry for the map and lighting grid cells |
| `crates/core/src/analog.rs`, `actuation.rs` | Key binding `02:12`/`02:92`, reading and writing thresholds |
| `crates/core/src/profiles.rs` | Slots `05:xx`, profile snapshot, diff and write in one batch |
| `crates/core/src/rapid.rs` | RT engine: depth stream parsing, thresholds, key repeat, Fn layer |
| `crates/core/src/lighting.rs` | Effect encoding from protocol templates |
| `crates/probe` | `razer-probe` — CLI for protocol research |
| `app/src-tauri` | Tauri 2: device, background thread, tray, autostart, engine on Windows (`engine.rs`) |
| `app/src-tauri/src/profiles.rs` | App profiles on top of slots: import, loading into profile 0, management |
| `app/src` | Vue: tabs, keyboard map, locales `locales/*.json` (shared with Rust) |

Protocol and firmware per model — [devices/](devices/README.md).

## Rules

- **Flash wears out.** Write to flash only on an explicit user command
  ("Save", floppy) and in a batch. Live changes go to volatile memory
  (profile `0`).
- **Synapse is running — write nothing.**
- **Do not copy Razer code.** Synapse bundles are only a protocol reference.
- **Do not bloat the scaffolding:** a trait or a new crate appears with the second implementation
  (second model, second OS).
- New strings go into both locales, `ru.json` and `en.json`.

## Pull requests

- One branch per change, from a fresh `master`.
- `cargo test` and `bun run lint` pass; changes to the protocol are checked on a live
  keyboard (`--ignored` tests) or say they were not.
- Commit messages: `<area>: <what it does>` in the imperative, e.g.
  `settings: save profiles compactly`. Title under 50 characters; a body only when the
  *why* is not obvious.
- New UI strings go into both locales; user-facing docs change in both `docs/en` and `docs/ru`.

## New keyboard model

Lighting, the press-point scale and the depth stream come from the description;
layout and key bindings are still shared with the Huntsman V2 Analog.

1. **Recon.** `razer-probe list` — HID interfaces; the control one is a 91-byte feature
   report (MI_03 on the V2 Analog). `razer-probe info` — firmware and mode.
   `razer-probe get …` — read-only commands. The PID is hardcoded in `crates/probe/src/main.rs`;
   change it for the recon.
2. **`crates/core/devices/<vid>-<pid>.json`** — name, `vid`, `pid`,
   `control_interface`, `tid` (transaction id), `lighting.protocol` and `led`,
   list of supported effects, `lamp_array` (if there is HID LampArray), `analog` (below)
   and `verified` once checked on hardware: until then the app writes only the keyboard's
   RAM, and only with "Write to keyboards not yet checked on hardware" on.
   Add the file to `DEVICES` in `crates/core/src/devices.rs`.
3. **Lighting protocol.** If the commands or effect bytes differ — a new
   `protocols/<name>.json` and a line in `PROTOCOLS`. An effect template is the bytes after
   `profile led` with the substitutions `{rgb1}`, `{rgb2}`, `{dir}`, `{speed}`.
4. **Layout and `fwID`** — `keymap.rs` and `layout.rs` are currently shared by all. For
   another layout they will have to become part of the model description.
5. **Analog** (`analog` in the description, `analog.rs`): `02:12` thresholds and depth
   0..255 map onto `zero_mm..full_mm`; `range` is what the window offers, `hw_range` where
   the firmware clamps in HW mode; `depth` is the MI_01 collection and report id of the
   depth stream, `wide` for a u16 depth. `firmware_rapid` — the firmware runs Rapid Trigger
   and Snap Tap in HW mode (`02:1A`/`02:21`, `trigger.rs`); in driver mode the host engine
   does it for every model.
6. A fake in `fake.rs` if the model answers differently; an `--ignored` test on hardware.
7. **Docs** — a model folder in `docs/en/devices/` and `docs/ru/devices/` (`protocol.md`, and `firmware.md` if the
   firmware is analyzed) and a line in the [model overview](devices/README.md).

An unknown Razer keyboard shows in the window as "This Razer keyboard model is not supported (PID …)".

## New lighting effect

Template in `protocols/*.json` → name in the model's `effects` → strings
`lighting.groups`/`types` in the locales. The UI builds the parameters (colors,
direction, speed) from the template itself.

## Where to look next

Game mode and the gamepad will need reading Synapse again:

- device logic bundles are cached in
  `%LOCALAPPDATA%\Razer\RazerAppEngine\User Data\Default\Service Worker\CacheStorage\…`
  (URL `apps.razer.com/synapse/products/614/`): `rzDevice25.*.js` — setters and
  getters, `3159.*.js` — the analog command table, `6737.*.js` — middleware
  (profiles, OBM, UI → FW conversion);
- logs — `%LOCALAPPDATA%\Razer\RazerAppEngine\User Data\Logs\products_614_*.log`
  (the `appEngine value` line shows what Synapse hands to its engine);
- the Synapse host engine — `CommonDLL\mapping_engine.dll`.

## Known issues

- The window hangs on the close button while actuation is being read.
- `reg.exe` is called on the main thread from the tray menu.
- A repeated launch with `--tray` shows the window of the running copy.
- Autostart points to the old path after the exe is moved.
- The selection frame grabs the case trim if started at the map edge.
- Lighting: "Apply" without a keyboard does not validate the effect; the draft is lost on
  reconnect; one broken parameter resets the whole `settings.json`.
- RT: on a hard kill of the process the keyboard is silent until reconnect.
