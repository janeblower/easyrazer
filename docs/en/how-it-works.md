# How it works

[Русская версия](../ru/how-it-works.md)

## Compared to Synapse

Measured on the same PC: Synapse 4.0.827 right after launch, EasyRazer 0.2.3 in the
tray in driver mode; HW mode needs less. In the tray the window is closed, so no WebView2
runs; opening it adds 6 × WebView2 processes, ~350 MB.

| | Synapse 4 | EasyRazer |
|---|---|---|
| Stack | Electron (`RazerAppEngine`) plus Windows services | Rust core, Tauri 2, Vue UI on the system WebView2 |
| Install | installer, ~1.2 GB on disk | one 8 MB `.exe`, no installer |
| Processes | 13: 11 × `RazerAppEngine`, Elevation Service, Game Manager Service | 1: `easyrazer` |
| RAM, working set / private | ~1.75 GB / ~1.07 GB | ~33 MB / ~8 MB in the tray (+200–300 MB for WebView2 while the window is open) |
| Network | loads device modules from `apps.razer.com` | none, works offline |
| Who detects key presses | the host, always: Synapse sets every key to `0/0` and runs the keyboard in driver mode | the keyboard firmware by default (HW mode); the host only in driver mode, when you want Rapid Trigger |

Synapse still has game mode and gamepad emulation; both are on the roadmap.

## HW mode and driver mode

In HW mode the keyboard types on its own, exactly as with no software installed:
no host in the input path, no extra latency, nothing to crash. EasyRazer switches to
driver mode only when you need Rapid Trigger — the Huntsman V2 Analog has no hardware
RT, so Synapse computes it on the host too. EasyRazer puts the keyboard back into HW
mode when it exits, fails, or sees Synapse start.

The trade-off: in HW mode the firmware clamps actuation to 1.62–3.56 mm; the full
1.5–3.6 mm range needs driver mode.

## Admin windows, lock screen, UAC

Windows drops keys that an ordinary app injects into windows run as administrator and
into the lock screen or UAC prompts. While one of those has focus, EasyRazer hands
typing back to the firmware (HW mode) and returns to driver mode afterwards; Rapid
Trigger does not work there. Each switch takes about 25 ms. Ctrl+Alt+Del accepts only
a real keyboard, so it does nothing in driver mode.

Injecting through Razer's kernel driver would lift these limits, but it was rejected as
too troublesome and unsafe.

## Flash wear

The keyboard's MCU (STM32L4+) is rated for 10,000 erases per 4 KB flash page. The
firmware has no wear levelling and no erase counter, and any write, even 4 bytes,
erases a whole page. So EasyRazer:

- applies changes to the keyboard's RAM; flash is written only by an explicit
  "Write" or the floppy icon, with an optional confirmation;
- writes a profile slot as one batch, and only the keys that differ from what is
  already stored;
- keeps Rapid Trigger and the custom colour layout in the app, never in flash.

Firmware analysis: [firmware.md](devices/huntsman-v2-analog/firmware.md).
